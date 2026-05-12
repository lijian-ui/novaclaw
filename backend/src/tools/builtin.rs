use serde_json::json;
use std::path::{Path, PathBuf};

use super::registry::ToolDef;
use super::registry::ToolRegistry;

/// 解析文件路径
/// 如果是相对路径，将其解析到工作目录下
/// 如果是绝对路径，直接返回
fn resolve_path(path_str: &str) -> PathBuf {
    let path = Path::new(path_str);
    
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        crate::config::get_workspace_dir().join(path)
    }
}

/// 查询预处理模块
/// 提升搜索质量的预处理逻辑
fn preprocess_query(query: &str, _context: Option<&str>) -> String {
    let mut processed = query.to_string();
    
    // 1. 去除首尾空格
    processed = processed.trim().to_string();
    
    // 2. 去除常见停用词（中文）
    let stop_words = ["的", "是", "在", "有", "和", "了", "我", "你", "他", "她", "它", "这", "那", "什么", "怎么", "如何", "为什么", "哪个", "哪些", "一个", "一些", "所有", "每个", "没有", "可以", "会", "能", "应该", "需要", "不要", "不是", "不会", "不能", "不要", "无法", "可能", "可能会", "可能是", "可能有"];
    for word in stop_words.iter() {
        processed = processed.replace(word, "");
    }
    
    // 3. 去除多余空格
    processed = processed.split_whitespace().collect::<Vec<&str>>().join(" ");
    
    // 4. 限制长度（最大256字符）
    if processed.len() > 256 {
        processed = processed[..256].to_string();
    }
    
    processed
}

/// 搜索结果结构
#[derive(Debug, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
    relevance: f64,
}

/// 解析并排序搜索结果
fn parse_and_rank(raw_results: &serde_json::Value, query: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    
    // 解析 DuckDuckGo 即时回答
    if let Some(abstract_text) = raw_results["AbstractText"].as_str() {
        if !abstract_text.is_empty() {
            let source = raw_results["AbstractSource"].as_str().unwrap_or("");
            let url = raw_results["AbstractURL"].as_str().unwrap_or("");
            
            results.push(SearchResult {
                title: source.to_string(),
                url: url.to_string(),
                snippet: abstract_text.to_string(),
                relevance: 1.0, // 即时回答相关性最高
            });
        }
    }
    
    // 解析搜索结果列表
    if let Some(results_array) = raw_results["Results"].as_array() {
        for (idx, result) in results_array.iter().enumerate() {
            if let (Some(text), Some(first_url)) = (
                result["Text"].as_str(),
                result["FirstURL"].as_str()
            ) {
                let snippet = result["Description"].as_str().unwrap_or(text);
                
                // 计算相关性得分
                let mut relevance = 0.5;
                if text.to_lowercase().contains(&query.to_lowercase()) {
                    relevance += 0.3;
                }
                // 位置越靠前，相关性越高
                relevance -= idx as f64 * 0.05;
                
                results.push(SearchResult {
                    title: text.to_string(),
                    url: first_url.to_string(),
                    snippet: snippet.to_string(),
                    relevance: relevance.max(0.1),
                });
            }
        }
    }
    
    // 解析相关话题
    if let Some(related_topics) = raw_results["RelatedTopics"].as_array() {
        for result in related_topics.iter() {
            if let (Some(text), Some(first_url)) = (
                result["Text"].as_str(),
                result["FirstURL"].as_str()
            ) {
                let snippet = result["Description"].as_str().unwrap_or(text);
                
                results.push(SearchResult {
                    title: text.to_string(),
                    url: first_url.to_string(),
                    snippet: snippet.to_string(),
                    relevance: 0.3, // 相关话题相关性较低
                });
            }
        }
    }
    
    // 去重处理
    results.dedup_by(|a, b| a.url == b.url);
    
    // 按相关性排序
    results.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
    
    // 返回前10条结果
    results.into_iter().take(10).collect()
}

/// 注册所有内置工具
/// 所有需要访问全局状态的数据（api key、memory_store、skills_loader）均从外部传入，
/// 避免在 handler 内部调用 APP_STATE.blocking_read()（在 tokio worker 上会 panic）
pub fn register_all(
    registry: &mut ToolRegistry,
    tinyfish_api_key: Option<String>,
    tavily_api_key: Option<String>,
    memory_store: crate::memory::store::MemoryStore,
    skills_loader: crate::skills::loader::SkillsLoader,
) {
    let rt = tokio::runtime::Handle::current();
    let registry_clone = registry.clone();
    // 将 memory_store 和 skills_loader 包装为 Arc，安全地传入多个 handler 闭包
    let memory_store = std::sync::Arc::new(memory_store);
    let skills_loader = std::sync::Arc::new(skills_loader);

    // 注册时不需要阻塞等待，使用 spawn_blocking 方式
    // 由于初始化在 tokio 上下文中，可以直接 block_on
    std::thread::spawn(move || {
        rt.block_on(async move {
            // read_file tool
            registry_clone.register(ToolDef {
                name: "read_file".to_string(),
                description: "Read file content from the filesystem".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path (relative paths resolve to workspace, absolute paths used directly)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Starting line number"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of lines to read"
                        }
                    },
                    "required": ["path"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("缺少 path 参数")?;
                    let resolved_path = resolve_path(path);
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("读取文件失败: {}", e))?;

                    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

                    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
                    Ok(lines.join("\n"))
                }),
            }).await;

            // write_file tool
            registry_clone.register(ToolDef {
                name: "write_file".to_string(),
                description: "Write content to a file (auto-creates directories)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path (relative paths resolve to workspace, absolute paths used directly)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("缺少 path 参数")?;
                    let content = args["content"].as_str().ok_or("缺少 content 参数")?;

                    let resolved_path = resolve_path(path);

                    if let Some(parent) = resolved_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("创建目录失败: {}", e))?;
                    }
                    std::fs::write(&resolved_path, content)
                        .map_err(|e| format!("写入文件失败: {}", e))?;

                    Ok(format!("成功写入文件: {}", resolved_path.display()))
                }),
            }).await;

            // edit_file tool
            registry_clone.register(ToolDef {
                name: "edit_file".to_string(),
                description: "Edit a file by finding and replacing text".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path (relative paths resolve to workspace, absolute paths used directly)"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "Text to search for and replace"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "New text to replace with"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("缺少 path 参数")?;
                    let old_str = args["old_string"].as_str().ok_or("缺少 old_string 参数")?;
                    let new_str = args["new_string"].as_str().ok_or("缺少 new_string 参数")?;

                    let resolved_path = resolve_path(path);
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("读取文件失败: {}", e))?;

                    if !content.contains(old_str) {
                        return Err(format!("未找到要替换的内容"));
                    }

                    // 只替换第一次出现
                    let new_content = content.replacen(old_str, new_str, 1);
                    std::fs::write(&resolved_path, &new_content)
                        .map_err(|e| format!("写入文件失败: {}", e))?;

                    Ok(format!("成功编辑文件: {}", resolved_path.display()))
                }),
            }).await;

            // glob file search tool
            registry_clone.register(ToolDef {
                name: "glob".to_string(),
                description: "Search files using glob patterns".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern, e.g. **/*.rs"
                        },
                        "path": {
                            "type": "string",
                            "description": "Root directory to search (relative paths resolve to workspace, absolute paths used directly); defaults to workspace"
                        }
                    },
                    "required": ["pattern"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let pattern = args["pattern"].as_str().ok_or("缺少 pattern 参数")?;
                    let base = args["path"].as_str().unwrap_or(".");

                    let resolved_base = resolve_path(base);

                    let glob_pattern = if resolved_base.to_string_lossy().ends_with('/') || resolved_base.to_string_lossy().ends_with('\\') {
                        format!("{}{}", resolved_base.display(), pattern)
                    } else {
                        format!("{}/{}", resolved_base.display(), pattern)
                    };

                    match glob::glob(&glob_pattern) {
                        Ok(entries) => {
                            let mut results = Vec::new();
                            for entry in entries.flatten() {
                                results.push(entry.display().to_string());
                                if results.len() >= 200 {
                                    results.push("...(结果已截断)".to_string());
                                    break;
                                }
                            }
                            if results.is_empty() {
                                Ok("未找到匹配的文件".to_string())
                            } else {
                                Ok(results.join("\n"))
                            }
                        }
                        Err(e) => Err(format!("glob 模式错误: {}", e)),
                    }
                }),
            }).await;

            // grep content search tool
            registry_clone.register(ToolDef {
                name: "grep".to_string(),
                description: "Search for text patterns in files".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search (relative paths resolve to workspace, absolute paths used directly); defaults to workspace"
                        },
                        "include": {
                            "type": "string",
                            "description": "File filter pattern, e.g. *.rs"
                        }
                    },
                    "required": ["pattern"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let pattern = args["pattern"].as_str().ok_or("缺少 pattern 参数")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    let include = args["include"].as_str();

                    let resolved_base = resolve_path(base);

                    let re = regex::RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .multi_line(true)
                        .build()
                        .map_err(|e| format!("正则表达式错误: {}", e))?;

                    let mut results = Vec::new();
                    let walker = walkdir::WalkDir::new(resolved_base).max_depth(20);

                    for entry in walker.into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_dir() {
                            continue;
                        }

                        if let Some(ref inc) = include {
                            if let Some(name) = entry.file_name().to_str() {
                                if !glob_match(name, inc) {
                                    continue;
                                }
                            }
                        }

                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            for (line_no, line) in content.lines().enumerate() {
                                if re.is_match(line) {
                                    results.push(format!(
                                        "{}:{}: {}",
                                        entry.path().display(),
                                        line_no + 1,
                                        line.trim()
                                    ));
                                    if results.len() >= 100 {
                                        break;
                                    }
                                }
                            }
                        }
                        if results.len() >= 100 {
                            break;
                        }
                    }

                    if results.is_empty() {
                        Ok("未找到匹配内容".to_string())
                    } else {
                        Ok(results.join("\n"))
                    }
                }),
            }).await;

            // memory tool
            let memory_store_for_memory = memory_store.clone();
            registry_clone.register(ToolDef {
                name: "memory".to_string(),
                description: "Persistent memory management: add / query / remove".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["add", "query", "remove"],
                            "description": "Action type"
                        },
                        "content": {
                            "type": "string",
                            "description": "Memory content (used for add/remove)"
                        },
                        "query": {
                            "type": "string",
                            "description": "Search keyword (used for query)"
                        }
                    },
                    "required": ["action"]
                }),
                handler: std::sync::Arc::new(move |args: serde_json::Value| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("缺少 action 参数")?;
                    match action {
                        "add" => {
                            let content = args["content"].as_str().ok_or("缺少 content 参数")?;
                            let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("general");
                            memory_store_for_memory.add_memory(content, category)
                                .map_err(|e| format!("记忆添加失败: {}", e))?;
                            Ok("记忆已保存".to_string())
                        }
                        "query" => {
                            let query = args["query"].as_str().unwrap_or("");
                            let memories = memory_store_for_memory.query_memories(query)
                                .map_err(|e| format!("记忆查询失败: {}", e))?;
                            if memories.is_empty() {
                                Ok("未找到相关记忆".to_string())
                            } else {
                                Ok(memories.join("\n---\n"))
                            }
                        }
                        "remove" => {
                            let content = args["content"].as_str().ok_or("缺少 content 参数")?;
                            memory_store_for_memory.remove_memory(content)
                                .map_err(|e| format!("记忆删除失败: {}", e))?;
                            Ok("记忆已删除".to_string())
                        }
                        _ => Err(format!("未知操作: {}", action)),
                    }
                }),
            }).await;

            // session_search tool
            registry_clone.register(ToolDef {
                name: "session_search".to_string(),
                description: "Search historical session messages".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search keyword"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results"
                        }
                    },
                    "required": ["query"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let query = args["query"].as_str().ok_or("缺少 query 参数")?;
                    let _limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                    // 简单的文本匹配搜索
                    Ok(format!("搜索 '{}' 完成（历史搜索功能开发中）", query))
                }),
            }).await;

            // web_search 工具 - 多后端搜索（handler 内开新线程+独立 Runtime，不阻塞 tokio worker）
            let web_search_client = std::sync::Arc::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .danger_accept_invalid_certs(true)
                    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                    .build()
                    .map_err(|e| e.to_string())
                    .unwrap_or_default()
            );
            // 直接使用外部传入的配置参数，完全避免在此处访问 APP_STATE（防止死锁）
            let web_search_tinyfish_key: std::sync::Arc<std::sync::Mutex<Option<String>>> =
                std::sync::Arc::new(std::sync::Mutex::new(tinyfish_api_key.clone()));
            let web_search_tavily_key: std::sync::Arc<std::sync::Mutex<Option<String>>> =
                std::sync::Arc::new(std::sync::Mutex::new(tavily_api_key.clone()));
            registry_clone.register(ToolDef {
                name: "web_search".to_string(),
                description: "Search the web for up-to-date information".to_string(),
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
                handler: std::sync::Arc::new(move |args: serde_json::Value| -> Result<String, String> {
                    let query = args["query"].as_str().ok_or("缺少 query 参数")?.to_string();
                    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                    let encoded_query = urlencoding::encode(&query).to_string();
                    let client = web_search_client.clone();

                    // 从 Arc<Mutex<>> 中读取配置，完全避免访问 tokio RwLock
                    let tinyfish_api_key = web_search_tinyfish_key.lock()
                        .map(|g| g.clone())
                        .unwrap_or(None);
                    let tavily_api_key = web_search_tavily_key.lock()
                        .map(|g| g.clone())
                        .unwrap_or(None);

                    let result = std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new()
                            .map_err(|e| format!("创建运行时失败: {}", e))?;

                        rt.block_on(async move {
                            let mut last_error = String::new();
                            let mut html = String::new();
                            let mut json_result = String::new();
                            let mut search_source = String::new();

                            // 1. DuckDuckGo
                            let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);
                            match client.post(&url)
                                .header("Content-Type", "application/x-www-form-urlencoded")
                                .body(format!("q={}", encoded_query))
                                .send().await
                            {
                                Ok(resp) => {
                                    let status = resp.status();
                                    match resp.text().await {
                                        Ok(text) => {
                                            html = text;
                                            search_source = "DuckDuckGo".to_string();
                                        }
                                        Err(e) => last_error = format!("DuckDuckGo 读取响应失败 (status={}): {}", status, e),
                                    }
                                }
                                Err(e) => last_error = format!("DuckDuckGo 请求失败: {}", e),
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
                                            Err(e) => last_error = format!("TinyFish 读取响应失败: {}", e),
                                        },
                                        Err(e) => last_error = format!("TinyFish 请求失败: {}", e),
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
                                    match client.post("https://api.tavily.com/search")
                                        .header("Authorization", format!("Bearer {}", key))
                                        .header("Content-Type", "application/json")
                                        .json(&body)
                                        .send().await
                                    {
                                        Ok(resp) => match resp.text().await {
                                            Ok(text) => {
                                                json_result = text;
                                                search_source = "Tavily".to_string();
                                            }
                                            Err(e) => last_error = format!("Tavily 读取响应失败: {}", e),
                                        },
                                        Err(e) => last_error = format!("Tavily 请求失败: {}", e),
                                    }
                                }
                            }

                            // 结果处理
                            let mut results: Vec<SearchResult> = Vec::new();

                            // JSON 解析（TinyFish / Tavily）
                            if !json_result.is_empty() {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_result) {
                                    if let Some(items) = json["results"].as_array() {
                                        for item in items {
                                            let snippet = item["snippet"].as_str()
                                                .or_else(|| item["content"].as_str())
                                                .unwrap_or("").to_string();
                                            results.push(SearchResult {
                                                title: item["title"].as_str().unwrap_or("").to_string(),
                                                url: item["url"].as_str().unwrap_or("").to_string(),
                                                snippet,
                                                relevance: 1.0 - (results.len() as f64 * 0.05),
                                            });
                                            if results.len() >= 10 { break; }
                                        }
                                    }
                                }
                            }

                            // DuckDuckGo HTML 解析
                            if results.is_empty() && !html.is_empty() && html.contains("result__a") {
                                let mut pos = 0;
                                while let Some(link_start) = html[pos..].find("class=\"result__a\"") {
                                    let actual_start = pos + link_start;
                                    let href_start = html[actual_start..].find("href=\"")
                                        .map(|i| actual_start + i + 6).unwrap_or(actual_start);
                                    let href_end = html[href_start..].find('\"')
                                        .map(|i| href_start + i).unwrap_or(href_start);
                                    let url = &html[href_start..href_end];
                                    let title_start = html[href_end..].find('>')
                                        .map(|i| href_end + i + 1).unwrap_or(href_end);
                                    let title_end = html[title_start..].find("</a>")
                                        .map(|i| title_start + i).unwrap_or(title_start);
                                    let snippet_start = html[title_end..].find("class=\"result__snippet\"")
                                        .and_then(|i| {
                                            let after_class = title_end + i;
                                            html[after_class..].find('>').map(|j| after_class + j + 1)
                                        }).unwrap_or(title_end);
                                    let snippet_end = html[snippet_start..].find("</a>")
                                        .or_else(|| html[snippet_start..].find("</span>"))
                                        .map(|i| snippet_start + i).unwrap_or_else(|| html.len());

                                    results.push(SearchResult {
                                        title: html_unescape(&html[title_start..title_end]).trim().to_string(),
                                        url: html_unescape(url.trim()).to_string(),
                                        snippet: html_unescape(&html[snippet_start..snippet_end]).trim().to_string(),
                                        relevance: 1.0 - (results.len() as f64 * 0.05),
                                    });
                                    pos = snippet_end;
                                    if results.len() >= 10 { break; }
                                }
                            }

                            if results.is_empty() {
                                Err(if last_error.is_empty() {
                                    format!("未找到与 '{}' 相关的搜索结果", query)
                                } else {
                                    format!("搜索失败: {}", last_error)
                                })
                            } else {
                                Ok(format_results(&results, count, &query, &search_source))
                            }
                        })
                    }).join()
                    .map_err(|_| "搜索线程崩溃".to_string())??;

                    Ok(result)
                }),
            }).await;

            // skills_list tool - list all available skills
            let skills_loader_for_list = skills_loader.clone();
            registry_clone.register(ToolDef {
                name: "skills_list".to_string(),
                description: "List all available skills with name and description".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                handler: std::sync::Arc::new(move |_args: serde_json::Value| -> Result<String, String> {
                    let skills = skills_loader_for_list.list_skills();
                    if skills.is_empty() {
                        return Ok("{\"skills\": []}".to_string());
                    }
                    let result: Vec<serde_json::Value> = skills.iter().map(|s| {
                        serde_json::json!({
                            "name": s.name,
                            "description": s.description,
                            "version": s.version,
                            "enabled": s.enabled,
                        })
                    }).collect();
                    Ok(serde_json::json!({ "skills": result }).to_string())
                }),
            }).await;

            // skill_view tool - view full details of a specific skill
            let skills_loader_for_view = skills_loader.clone();
            registry_clone.register(ToolDef {
                name: "skill_view".to_string(),
                description: "View the full content of a specific skill. Use skills_list first to see available skills".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the skill to view"
                        }
                    },
                    "required": ["name"]
                }),
                handler: std::sync::Arc::new(move |args: serde_json::Value| -> Result<String, String> {
                    let name = args["name"].as_str().ok_or("缺少 name 参数")?;
                    match skills_loader_for_view.get_skill(name) {
                        Some(skill) => {
                            Ok(serde_json::json!({
                                "name": skill.name,
                                "description": skill.description,
                                "version": skill.version,
                                "content": skill.content,
                            }).to_string())
                        }
                        None => {
                            let available: Vec<String> = skills_loader_for_view.list_skills()
                                .iter().map(|s| s.name.clone()).collect();
                            Err(format!("技能 '{}' 未找到。可用技能: {}", name, available.join(", ")))
                        }
                    }
                }),
            }).await;

            // todo tool
            registry_clone.register(ToolDef {
                name: "todo".to_string(),
                description: "Task management: add / list / done / remove".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["add", "list", "done", "remove"],
                            "description": "Action type"
                        },
                        "title": {
                            "type": "string",
                            "description": "Task title"
                        },
                        "id": {
                            "type": "string",
                            "description": "Task ID (used for done/remove)"
                        }
                    },
                    "required": ["action"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("缺少 action 参数")?;
                    match action {
                        "add" => {
                            let title = args["title"].as_str().unwrap_or("未命名任务");
                            Ok(format!("任务已添加: {}", title))
                        }
                        "list" => Ok("暂无待办任务".to_string()),
                        "done" => Ok("任务已标记完成".to_string()),
                        "remove" => Ok("任务已删除".to_string()),
                        _ => Err(format!("未知操作: {}", action)),
                    }
                }),
            }).await;

            tracing::info!("内置工具注册完成");
        });
    })
    .join()
    .ok();
}

/// 简单 glob 匹配
fn glob_match(name: &str, pattern: &str) -> bool {
    let pattern = pattern.replace("*", "");
    name.contains(&pattern)
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
        return format!("未找到与 '{}' 相关的搜索结果", query);
    }

    let mut output = Vec::new();
    let source_tag = if source.is_empty() { String::new() } else { format!(" - 来源: {}", source) };
    output.push(format!("🔍 搜索结果（共 {} 条）{}", results.len(), source_tag));
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
