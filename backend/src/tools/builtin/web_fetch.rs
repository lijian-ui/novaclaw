use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 web_fetch 工具
pub async fn register(registry: &ToolRegistry) {
    let http_client = std::sync::Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default(),
    );

    registry
        .register(ToolDef {
                        name: "web_fetch".to_string(),
            display_name: "抓取网页".to_string(),
            description:
                "Fetch and read the full content of a web page by URL. Use this after web_search to read the actual content of interesting results. Returns the page content as text."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch and read"
                    },
                    "max_length": {
                        "type": "integer",
                        "description": "Maximum characters to return (default 10000, max 50000)"
                    }
                },
                "required": ["url"]
            }),
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>|
                 -> Result<String, String> {
                    let url = args["url"].as_str().ok_or("Missing 'url' parameter")?;
                    let max_length = args["max_length"].as_u64().unwrap_or(10000).min(50000) as usize;

                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let resp = http_client
                            .get(url)
                            .send()
                            .await
                            .map_err(|e| format!("请求失败: {}", e))?;

                        let status = resp.status();
                        if !status.is_success() {
                            return Err(format!("HTTP {}: {}", status.as_u16(), url));
                        }

                        let content_type = resp
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string();

                        let bytes = resp
                            .bytes()
                            .await
                            .map_err(|e| format!("读取响应失败: {}", e))?;

                        let text = if content_type.contains("text/html") {
                            let html_str = String::from_utf8_lossy(&bytes);
                            extract_text_from_html(&html_str)
                        } else {
                            String::from_utf8_lossy(&bytes).to_string()
                        };

                        let text_len = text.len();
                        let truncated = if text_len > max_length {
                            format!("{}...\n\n[内容已截断，共 {} 字符，显示前 {} 字符]",
                                &text[..max_length], text_len, max_length)
                        } else {
                            text
                        };

                        Ok(json!({
                            "url": url,
                            "status": status.as_u16(),
                            "content": truncated,
                            "content_length": text_len,
                        }).to_string())
                    })
                },
            ),
        }).await;
}

/// 从 HTML 中提取纯文本（移除标签、保留段落结构）
fn extract_text_from_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script_or_style = 0; // 嵌套深度

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        if !in_tag {
            if c == '<' {
                in_tag = true;
                let rest: String = chars.iter().skip(i + 1).take(20).collect();
                let rest_lower = rest.to_lowercase();
                if rest_lower.starts_with("script") || rest_lower.starts_with("style") {
                    in_script_or_style = 1;
                } else if rest_lower.starts_with("/script") || rest_lower.starts_with("/style") {
                    in_script_or_style = 0;
                }
                // 关键块级标签后加换行
                if rest_lower.starts_with("br") || rest_lower.starts_with("/p") || rest_lower.starts_with("/div")
                    || rest_lower.starts_with("/h") || rest_lower.starts_with("/li") || rest_lower.starts_with("/tr")
                    || rest_lower.starts_with("/blockquote") || rest_lower.starts_with("hr") || rest_lower.starts_with("/pre")
                {
                    result.push('\n');
                }
                i += 1;
                continue;
            }
            if in_script_or_style > 0 {
                i += 1;
                continue;
            }
            result.push(c);
        } else {
            if c == '>' {
                in_tag = false;
            }
        }
        i += 1;
    }

    // 清理多余空白
    let lines: Vec<String> = result
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<&str>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();

    lines.join("\n")
}
