use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 搜索结果结构
#[derive(Debug, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
    #[allow(dead_code)]
    relevance: f64,
}

/// 注册 web_search 工具（DuckDuckGo / TinyFish / Tavily 多后端搜索）
pub async fn register(registry: &ToolRegistry, tinyfish_api_key: &Option<String>, tavily_api_key: &Option<String>) {
    let web_search_client = std::sync::Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .danger_accept_invalid_certs(true)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .build()
            .map_err(|e| e.to_string())
            .unwrap_or_default(),
    );
    let web_search_tinyfish_key: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(tinyfish_api_key.clone()));
    let web_search_tavily_key: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(tavily_api_key.clone()));

    registry
        .register(ToolDef {
            name: "web_search".to_string(),
            description:
                "Search the web (DuckDuckGo / TinyFish / Tavily). Params: query (required), count (optional, max 10)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of results (default 5, max 10)"
                    }
                },
                "required": ["query"]
            }),
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let query = args["query"]
                        .as_str()
                        .ok_or("Missing 'query' parameter - provide a search query")?
                        .to_string();
                    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                    let encoded_query = urlencoding::encode(&query).to_string();
                    let client = web_search_client.clone();

                    let tinyfish_api_key = web_search_tinyfish_key
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or(None);
                    let tavily_api_key = web_search_tavily_key
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or(None);

                    let result = std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new()
                            .map_err(|e| format!("Search engine runtime error: {}", e))?;

                        rt.block_on(async move {
                            let mut last_error = String::new();
                            let mut html = String::new();
                            let mut json_result = String::new();
                            let mut search_source = String::new();
                            let no_fallback = tinyfish_api_key
                                .as_ref()
                                .map_or(true, |k| k.is_empty())
                                && tavily_api_key.as_ref().map_or(true, |k| k.is_empty());

                            // 1. DuckDuckGo
                            let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);
                            match client
                                .post(&url)
                                .header("Content-Type", "application/x-www-form-urlencoded")
                                .body(format!("q={}", encoded_query))
                                .send()
                                .await
                            {
                                Ok(resp) => {
                                    let status = resp.status();
                                    match resp.text().await {
                                        Ok(text) => {
                                            html = text;
                                            search_source = "DuckDuckGo".to_string();
                                        }
                                        Err(e) => {
                                            last_error =
                                                format!("DuckDuckGo read failed (status={}): {}", status, e)
                                        }
                                    }
                                }
                                Err(e) => last_error = format!("DuckDuckGo request failed: {}", e),
                            }

                            // 2. TinyFish
                            if html.is_empty() {
                                if let Some(key) = tinyfish_api_key.filter(|k| !k.is_empty()) {
                                    let url = format!("https://api.search.tinyfish.ai?query={}", encoded_query);
                                    match client.get(&url).header("X-API-Key", &key).send().await {
                                        Ok(resp) => match resp.text().await {
                                            Ok(text) => {
                                                json_result = text;
                                                search_source = "TinyFish".to_string();
                                            }
                                            Err(e) => {
                                                last_error = format!("TinyFish read failed: {}", e)
                                            }
                                        },
                                        Err(e) => {
                                            last_error = format!("TinyFish request failed: {}", e)
                                        }
                                    }
                                }
                            }

                            // 3. Tavily
                            if html.is_empty() && json_result.is_empty() {
                                if let Some(key) = tavily_api_key.filter(|k| !k.is_empty()) {
                                    let body = serde_json::json!({
                                        "query": query,
                                        "search_depth": "basic",
                                        "include_answer": false,
                                        "include_images": false,
                                        "max_results": 5
                                    });
                                    match client
                                        .post("https://api.tavily.com/search")
                                        .header("Authorization", format!("Bearer {}", key))
                                        .header("Content-Type", "application/json")
                                        .json(&body)
                                        .send()
                                        .await
                                    {
                                        Ok(resp) => match resp.text().await {
                                            Ok(text) => {
                                                json_result = text;
                                                search_source = "Tavily".to_string();
                                            }
                                            Err(e) => {
                                                last_error = format!("Tavily read failed: {}", e)
                                            }
                                        },
                                        Err(e) => {
                                            last_error = format!("Tavily request failed: {}", e)
                                        }
                                    }
                                }
                            }

                            let mut results: Vec<SearchResult> = Vec::new();

                            // 解析 JSON (TinyFish / Tavily)
                            if !json_result.is_empty() {
                                if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&json_result)
                                {
                                    if let Some(items) = json["results"].as_array() {
                                        for item in items {
                                            let snippet = item["snippet"]
                                                .as_str()
                                                .or_else(|| item["content"].as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            results.push(SearchResult {
                                                title: item["title"]
                                                    .as_str()
                                                    .unwrap_or("")
                                                    .to_string(),
                                                url: item["url"]
                                                    .as_str()
                                                    .unwrap_or("")
                                                    .to_string(),
                                                snippet,
                                                relevance: 1.0 - (results.len() as f64 * 0.05),
                                            });
                                            if results.len() >= 10 {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            // DuckDuckGo HTML 解析
                            if results.is_empty() && !html.is_empty() && html.contains("result__a") {
                                let mut pos = 0;
                                while let Some(link_start) = html[pos..].find("class=\"result__a\"") {
                                    let actual_start = pos + link_start;
                                    let href_start = html[actual_start..]
                                        .find("href=\"")
                                        .map(|i| actual_start + i + 6)
                                        .unwrap_or(actual_start);
                                    let href_end = html[href_start..]
                                        .find('\"')
                                        .map(|i| href_start + i)
                                        .unwrap_or(href_start);
                                    let url = &html[href_start..href_end];
                                    let title_start = html[href_end..]
                                        .find('>')
                                        .map(|i| href_end + i + 1)
                                        .unwrap_or(href_end);
                                    let title_end = html[title_start..]
                                        .find("</a>")
                                        .map(|i| title_start + i)
                                        .unwrap_or(title_start);
                                    let snippet_start = html[title_end..]
                                        .find("class=\"result__snippet\"")
                                        .and_then(|i| {
                                            let after_class = title_end + i;
                                            html[after_class..]
                                                .find('>')
                                                .map(|j| after_class + j + 1)
                                        })
                                        .unwrap_or(title_end);
                                    let snippet_end = html[snippet_start..]
                                        .find("</a>")
                                        .or_else(|| html[snippet_start..].find("</span>"))
                                        .map(|i| snippet_start + i)
                                        .unwrap_or_else(|| html.len());

                                    results.push(SearchResult {
                                        title: html_unescape(&html[title_start..title_end])
                                            .trim()
                                            .to_string(),
                                        url: html_unescape(url.trim()).to_string(),
                                        snippet: html_unescape(&html[snippet_start..snippet_end])
                                            .trim()
                                            .to_string(),
                                        relevance: 1.0 - (results.len() as f64 * 0.05),
                                    });
                                    pos = snippet_end;
                                    if results.len() >= 10 {
                                        break;
                                    }
                                }
                            }

                            if results.is_empty() {
                                let msg = if last_error.is_empty() {
                                    format!("No search results for '{}'", query)
                                } else if no_fallback {
                                    format!(
                                        "{}\n提示：DuckDuckGo 可能被网络限制，请在设置中配置 TinyFish 或 Tavily 搜索 API Key 作为备用搜索引擎",
                                        last_error
                                    )
                                } else {
                                    format!("Search failed: {}", last_error)
                                };
                                Err(msg)
                            } else {
                                Ok(format_results(&results, count, &query, &search_source))
                            }
                        })
                    })
                    .join()
                    .map_err(|_| "Search engine thread crashed".to_string())??;

                    Ok(result)
                },
            ),
        })
        .await;
}

/// 简单的 HTML 实体解码（只处理常见实体）
fn html_unescape(s: &str) -> String {
    let mut result = s.to_string();
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&#39;", "'");
    result = result.replace("&#x27;", "'");
    result = result.replace("&#x2F;", "/");
    result = result.replace("&nbsp;", " ");
    result
}

/// 格式化搜索结果输出
fn format_results(results: &[SearchResult], count: usize, query: &str, source: &str) -> String {
    if results.is_empty() {
        return format!("No search results for '{}'", query);
    }

    let mut output = Vec::new();
    let source_tag = if source.is_empty() {
        String::new()
    } else {
        format!(" - Source: {}", source)
    };
    output.push(format!("🔍 Search results ({} total){}", results.len(), source_tag));
    output.push("---".to_string());

    for (idx, result) in results.iter().take(count).enumerate() {
        output.push(format!("{}. **{}**", idx + 1, result.title));
        output.push(format!("   📄 {}", result.url));
        if !result.snippet.is_empty() {
            let snippet: String = result.snippet.chars().take(200).collect();
            output.push(format!("   💡 {}", snippet));
        }
        output.push("".to_string());
    }

    output.join("\n")
}
