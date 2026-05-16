use serde_json::json;
use std::path::{Path, PathBuf};

use super::registry::ToolDef;
use super::registry::ToolRegistry;

/// 解析文件路径
/// 如果是相对路径，优先使用会话的工作目录；没有则使用全局默认 workspace
/// 如果是绝对路径，直接返回
fn resolve_path(path_str: &str, args: &serde_json::Value) -> PathBuf {
    let path = Path::new(path_str);
    
    if path.is_absolute() {
        return path.to_path_buf();
    }
    
    // 优先使用注入的 session workspace
    if let Some(ws) = args.get("_workspace").and_then(|v| v.as_str()) {
        return PathBuf::from(ws).join(path);
    }
    
    // 兜底：使用全局默认 workspace
    crate::config::get_workspace_dir().join(path)
}

/// 查询预处理模块
/// 提升搜索质量的预处理逻辑
#[allow(dead_code)]
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
    #[allow(dead_code)]
    relevance: f64,
}

/// 解析并排序搜索结果
#[allow(dead_code)]
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
    mcp_store: std::sync::Arc<tokio::sync::Mutex<crate::mcp::McpStore>>,
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
                description: "Read file content. Params: path (required), offset (optional line number), limit (optional max lines)".to_string(),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let resolved_path = resolve_path(path, &args);
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("Failed to read file: {}", e))?;

                    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

                    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
                    Ok(lines.join("\n"))
                }),
            }).await;

            // write_file tool
            registry_clone.register(ToolDef {
                name: "write_file".to_string(),
                description: "Write content to a file (auto-creates dirs). Params: path (required), content (required)".to_string(),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let content = args["content"].as_str().ok_or("Missing 'content' parameter")?;

                    let resolved_path = resolve_path(path, &args);

                    if let Some(parent) = resolved_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create directory: {}", e))?;
                    }
                    std::fs::write(&resolved_path, content)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    Ok(format!("Successfully wrote file: {}", resolved_path.display()))
                }),
            }).await;

            // edit_file tool
            registry_clone.register(ToolDef {
                name: "edit_file".to_string(),
                description: "Find and replace text in a file (1 replacement). Params: path (required), old_string (required), new_string (required)".to_string(),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let old_str = args["old_string"].as_str().ok_or("Missing 'old_string' parameter")?;
                    let new_str = args["new_string"].as_str().ok_or("Missing 'new_string' parameter")?;

                    let resolved_path = resolve_path(path, &args);
                    let content = std::fs::read_to_string(&resolved_path)
                        .map_err(|e| format!("Failed to read file: {}", e))?;

                    if !content.contains(old_str) {
                        return Err("Text not found: the 'old_string' does not exist in the file. Make sure to match exact content including whitespace.".to_string());
                    }

                    // 只替换第一次出现
                    let new_content = content.replacen(old_str, new_str, 1);
                    std::fs::write(&resolved_path, &new_content)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    Ok(format!("Successfully edited file: {}", resolved_path.display()))
                }),
            }).await;

            // glob file search tool
            registry_clone.register(ToolDef {
                name: "glob".to_string(),
                description: "Search files by glob pattern. Params: pattern (required, e.g. **/*.rs or **/*), path (optional directory)".to_string(),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let pattern = args["pattern"].as_str().ok_or("Missing 'pattern' parameter - provide a glob pattern like **/*.rs")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    // 处理 LLM 可能误传的路径别名，统一映射到工作目录根
                    let base = match base {
                        "" | "." | "workspace" | "workspace/" | "workspace\\" => ".",
                        other => other,
                    };

                    let resolved_base = resolve_path(base, &args);

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
                                    results.push("...(results truncated)".to_string());
                                    break;
                                }
                            }
                            if results.is_empty() {
                                Ok("No files matched the pattern".to_string())
                            } else {
                                Ok(results.join("\n"))
                            }
                        }
                        Err(e) => Err(format!("Invalid glob pattern: {}", e)),
                    }
                }),
            }).await;

            // grep content search tool
            registry_clone.register(ToolDef {
                name: "grep".to_string(),
                description: "Search text in files with regex. Pass a directory path as 'path' (not a glob pattern). Params: pattern (required), path (optional directory, e.g. '.' or 'scripts'), include (optional file filter like '*.rs')".to_string(),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let pattern = args["pattern"].as_str().ok_or("Missing 'pattern' parameter - provide a regex pattern to search for")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    // 处理 LLM 可能误传的路径别名，统一映射到工作目录根
                    let base = match base.trim_end_matches('*').trim_end_matches('/').trim_end_matches('\\') {
                        "" | "." | "workspace" | "workspace/" | "workspace\\" => ".",
                        other => other,
                    };
                    let include = args["include"].as_str();

                    let resolved_base = resolve_path(base, &args);

                    let re = regex::RegexBuilder::new(pattern)
                        .case_insensitive(true)
                        .multi_line(true)
                        .build()
                        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

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

                        // 使用 BufReader 逐行读取，避免大文件全量加载到内存
                        if let Ok(file) = std::fs::File::open(entry.path()) {
                            let mut reader = std::io::BufReader::new(file);
                            let mut line_no = 0usize;
                            loop {
                                let mut line = String::new();
                                match std::io::BufRead::read_line(&mut reader, &mut line) {
                                    Ok(0) => break, // EOF
                                    Ok(_) => {
                                        line_no += 1;
                                        let trimmed = line.trim_end();
                                        if re.is_match(trimmed) {
                                            results.push(format!(
                                                "{}:{}: {}",
                                                entry.path().display(),
                                                line_no,
                                                trimmed
                                            ));
                                            if results.len() >= 100 {
                                                break;
                                            }
                                        }
                                    }
                                    Err(_) => continue, // 跳过无法读取的行
                                }
                            }
                        }
                        if results.len() >= 100 {
                            break;
                        }
                    }

                    if results.is_empty() {
                        Ok("No matches found - try a different search term, pattern, or file path".to_string())
                    } else {
                        Ok(results.join("\n"))
                    }
                }),
            }).await;

            // memory tool
            let memory_store_for_memory = memory_store.clone();
            registry_clone.register(ToolDef {
                name: "memory".to_string(),
                description: "Persistent memory management. Params: action (required, enum: add/query/remove), content (required for add/remove), query (for find)".to_string(),
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
                handler: std::sync::Arc::new(move |args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("Missing 'action' parameter (add/query/remove)")?;
                    match action {
                        "add" => {
                            let content = args["content"].as_str().ok_or("Missing 'content' parameter for add action")?;
                            let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("general");
                            memory_store_for_memory.add_memory(content, category)
                                .map_err(|e| format!("Memory add failed: {}", e))?;
                            Ok("Memory saved".to_string())
                        }
                        "query" => {
                            let query = args["query"].as_str().unwrap_or("");
                            let memories = memory_store_for_memory.query_memories(query)
                                .map_err(|e| format!("Memory query failed: {}", e))?;
                            if memories.is_empty() {
                                Ok("No relevant memories found".to_string())
                            } else {
                                Ok(memories.join("\n---\n"))
                            }
                        }
                        "remove" => {
                            let content = args["content"].as_str().ok_or("Missing 'content' parameter for remove action")?;
                            memory_store_for_memory.remove_memory(content)
                                .map_err(|e| format!("Memory remove failed: {}", e))?;
                            Ok("Memory removed".to_string())
                        }
                        _ => Err(format!("Unknown action '{}'. Supported: add, query, remove", action)),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let query = args["query"].as_str().ok_or("Missing 'query' parameter")?;
                    let _limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                    // 简单的文本匹配搜索
                    Ok(format!("Search '{}' completed (session search feature in development)", query))
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
                description: "Search the web (DuckDuckGo / TinyFish / Tavily). Params: query (required), count (optional, max 10)".to_string(),
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
                handler: std::sync::Arc::new(move |args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let query = args["query"].as_str().ok_or("Missing 'query' parameter - provide a search query")?.to_string();
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
                            .map_err(|e| format!("Search engine runtime error: {}", e))?;

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
                                        Err(e) => last_error = format!("DuckDuckGo read failed (status={}): {}", status, e),
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
                                            Err(e) => last_error = format!("TinyFish read failed: {}", e),
                                        },
                                        Err(e) => last_error = format!("TinyFish request failed: {}", e),
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
                                            Err(e) => last_error = format!("Tavily read failed: {}", e),
                                        },
                                        Err(e) => last_error = format!("Tavily request failed: {}", e),
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
                                    format!("No search results for '{}'", query)
                                } else {
                                    format!("Search failed: {}", last_error)
                                })
                            } else {
                                Ok(format_results(&results, count, &query, &search_source))
                            }
                        })
                    }).join()
                    .map_err(|_| "Search engine thread crashed".to_string())??;

                    Ok(result)
                }),
            }).await;

            // skill_view tool - view full details of a specific skill
            let skills_loader_for_view = skills_loader.clone();
            registry_clone.register(ToolDef {
                name: "skill_view".to_string(),
                description: "View the full content of a specific skill. The skill_dir field tells you the absolute path to the skill folder, use it to reference scripts or assets. Available skills are listed in the system prompt.".to_string(),
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
                handler: std::sync::Arc::new(move |args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let name = args["name"].as_str().ok_or("Missing 'name' parameter - provide the skill name")?;
                    tracing::info!("[Skill] skill_view 加载技能: {}", name);
                    match skills_loader_for_view.get_skill(name) {
                        Some(skill) => {
                            // source_path 已经是技能目录（skills/<skill_name>），直接使用
                            // 兼容多种占位符格式：{SKILL_DIR} / ${SKILL_DIR} / ${HERMES_SKILL_DIR}
                            let raw = &skill.source_path;
                            // 根据操作系统归一化路径格式：Windows 用反斜杠，其他平台用正斜杠
                            let normalized_dir = if cfg!(target_os = "windows") {
                                raw.replace('/', "\\")
                            } else {
                                raw.replace('\\', "/")
                            };
                            let content = skill.content
                                .replace("{SKILL_DIR}", &normalized_dir)
                                .replace("${SKILL_DIR}", &normalized_dir)
                                .replace("${HERMES_SKILL_DIR}", &normalized_dir);
                            Ok(serde_json::json!({
                                "name": skill.name,
                                "description": skill.description,
                                "version": skill.version,
                                "content": content,
                                "skill_dir": normalized_dir,
                            }).to_string())
                        }
                        None => {
                            let available: Vec<String> = skills_loader_for_view.list_skills()
                                .iter().map(|s| s.name.clone()).collect();
                            Err(format!("Skill '{}' not found. Available skills: {}", name, available.join(", ")))
                        }
                    }
                }),
            }).await;

            // todo tool
            registry_clone.register(ToolDef {
                name: "todo".to_string(),
                description: "Simple task management. Params: action (required, enum: add/list/done/remove), title (for add), id (for done/remove)".to_string(),
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
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("Missing 'action' parameter (add/list/done/remove)")?;
                    match action {
                        "add" => {
                            let title = args["title"].as_str().unwrap_or("unnamed task");
                            Ok(format!("Task added: {}", title))
                        }
                        "list" => Ok("No pending tasks".to_string()),
                        "done" => Ok("Task marked as done".to_string()),
                        "remove" => Ok("Task removed".to_string()),
                        _ => Err(format!("Unknown action '{}'. Supported: add, list, done, remove", action)),
                    }
                }),
            }).await;

            // search_replace tool - batch find and replace across files
            registry_clone.register(ToolDef {
                name: "search_replace".to_string(),
                description: "Batch find and replace text across multiple files using regex. Params: pattern (required, regex), replacement (required), path (optional directory, default workspace), include (optional file filter like '*.rs' or '*.tsx')".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "replacement": {
                            "type": "string",
                            "description": "Replacement text"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in (defaults to workspace root)"
                        },
                        "include": {
                            "type": "string",
                            "description": "File filter, e.g. '*.rs' or '*.tsx'"
                        }
                    },
                    "required": ["pattern", "replacement"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let pattern = args["pattern"].as_str().ok_or("Missing 'pattern' parameter")?;
                    let replacement = args["replacement"].as_str().ok_or("Missing 'replacement' parameter")?;
                    let base = args["path"].as_str().unwrap_or(".");
                    let base = match base.trim_end_matches('*').trim_end_matches('/').trim_end_matches('\\') {
                        "" | "." | "workspace" => ".",
                        other => other,
                    };
                    let include = args["include"].as_str();

                    let resolved_base = resolve_path(base, &args);
                    let re = regex::RegexBuilder::new(pattern)
                        .multi_line(true)
                        .build()
                        .map_err(|e| format!("Invalid regex pattern: {}", e))?;

                    let walker = walkdir::WalkDir::new(&resolved_base).max_depth(20);
                    let mut total_changes = 0usize;
                    let mut changed_files: Vec<String> = Vec::new();

                    for entry in walker.into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_dir() { continue; }
                        if let Some(ref inc) = include {
                            if let Some(name) = entry.file_name().to_str() {
                                if !glob_match(name, inc) { continue; }
                            }
                        }
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if re.is_match(&content) {
                                let new_content = re.replace_all(&content, replacement);
                                if new_content != content {
                                    if std::fs::write(entry.path(), new_content.as_ref()).is_ok() {
                                        total_changes += 1;
                                        changed_files.push(entry.path().display().to_string());
                                    }
                                }
                            }
                        }
                    }

                    if changed_files.is_empty() {
                        Ok("No matches found - no files were modified".to_string())
                    } else {
                        Ok(format!(
                            "Replaced matches in {} file(s):\n{}",
                            total_changes,
                            changed_files.join("\n")
                        ))
                    }
                }),
            }).await;

            // list_dir tool - list directory contents
            registry_clone.register(ToolDef {
                name: "list_dir".to_string(),
                description: "List files and directories. Params: path (optional, default workspace). Returns directory entries with name, type, and size".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path (relative to workspace or absolute)"
                        },
                        "depth": {
                            "type": "integer",
                            "description": "Max recursion depth (default 1, use 0 for unlimited)"
                        }
                    },
                    "required": []
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let base = args["path"].as_str().unwrap_or(".");
                    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                    let resolved_base = resolve_path(base, &args);

                    if !resolved_base.exists() {
                        return Err(format!("Directory not found: {}", resolved_base.display()));
                    }
                    if !resolved_base.is_dir() {
                        return Err(format!("Not a directory: {}", resolved_base.display()));
                    }

                    let max_depth = if depth == 0 { usize::MAX } else { depth };
                    let walker = walkdir::WalkDir::new(&resolved_base).max_depth(max_depth);
                    let mut entries: Vec<String> = Vec::new();

                    for entry in walker.into_iter().filter_map(|e| e.ok()) {
                        let indent = entry.depth().saturating_sub(1);
                        let prefix = "  ".repeat(indent);
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') || name == "node_modules" { continue; }
                        let file_type = if entry.file_type().is_dir() { "📁" } else { "📄" };
                        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                        let size_str = if size > 1024 * 1024 {
                            format!(" ({:.1} MB)", size as f64 / 1024.0 / 1024.0)
                        } else if size > 1024 {
                            format!(" ({:.1} KB)", size as f64 / 1024.0)
                        } else if size > 0 {
                            format!(" ({} B)", size)
                        } else {
                            String::new()
                        };
                        entries.push(format!("{}{} {}{}", prefix, file_type, name, size_str));
                    }

                    if entries.is_empty() {
                        Ok("(empty directory)".to_string())
                    } else {
                        Ok(entries.join("\n"))
                    }
                }),
            }).await;

            // delete_file tool
            registry_clone.register(ToolDef {
                name: "delete_file".to_string(),
                description: "Delete a file or directory (recursive). Params: path (required)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File or directory path to delete"
                        }
                    },
                    "required": ["path"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let resolved = resolve_path(path_str, &args);

                    if !resolved.exists() {
                        return Err(format!("Path not found: {}", resolved.display()));
                    }

                    if resolved.is_dir() {
                        std::fs::remove_dir_all(&resolved)
                            .map_err(|e| format!("Failed to delete directory: {}", e))?;
                        Ok(format!("Deleted directory: {}", resolved.display()))
                    } else {
                        std::fs::remove_file(&resolved)
                            .map_err(|e| format!("Failed to delete file: {}", e))?;
                        Ok(format!("Deleted file: {}", resolved.display()))
                    }
                }),
            }).await;

            // rename_file tool
            registry_clone.register(ToolDef {
                name: "rename_file".to_string(),
                description: "Rename or move a file/directory. Params: path (required, source), new_path (required, destination)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Source file or directory path"
                        },
                        "new_path": {
                            "type": "string",
                            "description": "Destination file or directory path"
                        }
                    },
                    "required": ["path", "new_path"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let old_path = args["path"].as_str().ok_or("Missing 'path' parameter")?;
                    let new_path = args["new_path"].as_str().ok_or("Missing 'new_path' parameter")?;
                    let old_resolved = resolve_path(old_path, &args);
                    let new_resolved = resolve_path(new_path, &args);

                    if !old_resolved.exists() {
                        return Err(format!("Source not found: {}", old_resolved.display()));
                    }
                    if new_resolved.exists() {
                        return Err(format!("Destination already exists: {}", new_resolved.display()));
                    }

                    // Create parent directory if needed
                    if let Some(parent) = new_resolved.parent() {
                        if !parent.exists() {
                            std::fs::create_dir_all(parent)
                                .map_err(|e| format!("Failed to create destination directory: {}", e))?;
                        }
                    }

                    std::fs::rename(&old_resolved, &new_resolved)
                        .map_err(|e| format!("Failed to rename: {}", e))?;

                    Ok(format!("Renamed: {} -> {}", old_resolved.display(), new_resolved.display()))
                }),
            }).await;

            // apply_patch tool - apply unified diff
            registry_clone.register(ToolDef {
                name: "apply_patch".to_string(),
                description: "Apply a unified diff (patch) to files. The diff format is standard 'diff -u' output with ---/+++ headers and @@ hunks. Params: diff (required, the patch content)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "diff": {
                            "type": "string",
                            "description": "Unified diff content to apply"
                        }
                    },
                    "required": ["diff"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let diff = args["diff"].as_str().ok_or("Missing 'diff' parameter")?;
                    apply_unified_diff(diff, &args).map(|summary| {
                        format!("Patch applied successfully:\n{}", summary.join("\n"))
                    })
                }),
            }).await;

            // lsp tool - semantic code analysis
            registry_clone.register(ToolDef {
                name: "lsp".to_string(),
                description: "Semantic code analysis via language server. Supports: definition (find where something is defined), references (find all usages), diagnostics (get compile/lint errors), hover (get type info/documentation). Params: action (required), file (required), symbol (optional name), line/character (optional position)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["definition", "references", "diagnostics", "hover"],
                            "description": "Action to perform"
                        },
                        "file": {
                            "type": "string",
                            "description": "Path to the source file"
                        },
                        "symbol": {
                            "type": "string",
                            "description": "Symbol name to look up (used for definition/references/hover)"
                        },
                        "line": {
                            "type": "integer",
                            "description": "Line number (0-based, optional)"
                        },
                        "character": {
                            "type": "integer",
                            "description": "Character offset (0-based, optional)"
                        }
                    },
                    "required": ["action", "file"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("Missing 'action' parameter")?;
                    let file = args["file"].as_str().ok_or("Missing 'file' parameter")?;
                    let resolved_file = resolve_path(file, &args);
                    let symbol = args["symbol"].as_str();

                    if !resolved_file.exists() {
                        return Err(format!("File not found: {}", resolved_file.display()));
                    }

                    // Determine language server command from file extension
                    let ext = resolved_file.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let server_cmd: String = match ext {
                        "rs" => "rust-analyzer".to_string(),
                        "ts" | "tsx" | "js" | "jsx" => {
                            // 优先使用项目内 node_modules/.bin 中的版本
                            let node_bin = std::path::Path::new("node_modules/.bin/typescript-language-server");
                            if node_bin.exists() {
                                node_bin.to_string_lossy().to_string()
                            } else if cfg!(target_os = "windows") {
                                // Windows: 检查 node_modules/.bin/typescript-language-server.cmd
                                let node_bat = std::path::Path::new("node_modules/.bin/typescript-language-server.cmd");
                                if node_bat.exists() {
                                    node_bat.to_string_lossy().to_string()
                                } else {
                                    "typescript-language-server".to_string()
                                }
                            } else {
                                "typescript-language-server".to_string()
                            }
                        }
                        "py" => "pylsp".to_string(),
                        "go" => "gopls".to_string(),
                        "java" => "eclipse.jdt.ls".to_string(),
                        _ => return Err(format!("No language server available for .{} files. Try using grep or search_replace as fallback. Supported: .rs, .ts, .tsx, .js, .jsx, .py, .go, .java", ext)),
                    };

                    let content = std::fs::read_to_string(&resolved_file)
                        .map_err(|e| format!("Failed to read file: {}", e))?;

                    let file_path = resolved_file.to_string_lossy().to_string();
                    let root_uri = resolved_file.parent()
                        .map(|p| format!("file:///{}", p.to_string_lossy().replace('\\', "/")))
                        .unwrap_or_default();

                    let mut client = crate::tools::lsp::LspClient::new(&server_cmd);
                    client.start(&root_uri).map_err(|e| format!("LSP start failed: {}", e))?;

                    let (line, character) = if let (Some(l), Some(c)) = (
                        args.get("line").and_then(|v| v.as_u64()),
                        args.get("character").and_then(|v| v.as_u64())
                    ) {
                        (l as u32, c as u32)
                    } else if let Some(sym) = symbol {
                        crate::tools::lsp::find_position(&content, sym, None)
                    } else {
                        (0, 0)
                    };

                    let result = match action {
                        "definition" => {
                            let defs = client.goto_definition(&file_path, line, character)?;
                            if defs.is_empty() {
                                "No definition found".to_string()
                            } else {
                                let lines: Vec<String> = defs.iter().map(|d| {
                                    let uri = d.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                                    let range = d.get("range").or_else(|| d.get("targetRange"));
                                    let start = range.and_then(|r| r.get("start")).unwrap_or(&serde_json::Value::Null);
                                    let rl = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let rc = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0);
                                    format!("{}:{}:{}", uri, rl + 1, rc)
                                }).collect();
                                format!("Definition found at:\n{}", lines.join("\n"))
                            }
                        }
                        "references" => {
                            let refs = client.find_references(&file_path, line, character)?;
                            if refs.is_empty() {
                                "No references found".to_string()
                            } else {
                                let lines: Vec<String> = refs.iter().map(|r| {
                                    let uri = r.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                                    let start = r.get("range").and_then(|r| r.get("start")).unwrap_or(&serde_json::Value::Null);
                                    let rl = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let rc = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0);
                                    format!("{}:{}:{}", uri, rl + 1, rc)
                                }).collect();
                                format!("Found {} reference(s):\n{}", lines.len(), lines.join("\n"))
                            }
                        }
                        "diagnostics" => {
                            let diags = client.get_diagnostics(&file_path, &content)?;
                            if diags.is_empty() {
                                "No diagnostics - file looks clean".to_string()
                            } else {
                                let lines: Vec<String> = diags.iter().map(|d| {
                                    let msg = d.get("message").and_then(|v| v.as_str()).unwrap_or("");
                                    let severity = match d.get("severity").and_then(|v| v.as_u64()).unwrap_or(0) {
                                        1 => "ERROR",
                                        2 => "WARNING",
                                        3 => "INFO",
                                        4 => "HINT",
                                        _ => "UNKNOWN",
                                    };
                                    let range = d.get("range").unwrap_or(&serde_json::Value::Null);
                                    let start = range.get("start").unwrap_or(&serde_json::Value::Null);
                                    let rl = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let rc = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0);
                                    format!("{}:{} {}: {}", rl + 1, rc, severity, msg)
                                }).collect();
                                format!("Found {} diagnostic(s):\n{}", lines.len(), lines.join("\n"))
                            }
                        }
                        "hover" => {
                            let info = client.hover(&file_path, line, character)?;
                            if info.is_empty() {
                                "No information available at this position".to_string()
                            } else {
                                format!("```\n{}\n```", info)
                            }
                        }
                        _ => return Err(format!("Unknown action '{}'. Supported: definition, references, diagnostics, hover", action)),
                    };

                    client.shutdown();
                    Ok(result)
                }),
            }).await;

            // execute_command tool - execute shell commands via PTY
            registry_clone.register(ToolDef {
                name: "execute_command".to_string(),
                description: "Execute any shell command in the workspace directory. Pass only the raw command (e.g. 'dir', 'ls -la', 'npm run build'), do NOT wrap it in powershell/cmd/bash/shell invocation - the tool handles that automatically. Params: command (required), description (optional), timeout (optional, default 60s), workdir (optional)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute (e.g. 'npm run build', 'cargo test', 'python script.py')"
                        },
                        "description": {
                            "type": "string",
                            "description": "Clear explanation of what this command does (helps with safety review)"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Maximum execution time in seconds (default 60, max 300)"
                        },
                        "workdir": {
                            "type": "string",
                            "description": "Working directory (relative to workspace or absolute, defaults to workspace)"
                        }
                    },
                    "required": ["command"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let command = args["command"].as_str().ok_or("Missing 'command' parameter")?;
                    let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60).min(300);
                    let workdir_str = args.get("workdir").and_then(|v| v.as_str()).unwrap_or(".");
                    let resolved_workdir = resolve_path(workdir_str, &args);
                    tracing::info!("[Execute] 执行命令: {} | 工作目录: {}", command, resolved_workdir.display());

                    if !resolved_workdir.exists() {
                        return Err(format!("Working directory not found: {}", resolved_workdir.display()));
                    }

                    // 将发送器包装为回调，用于实时推送终端输出
                    let chunk_cb = chunk_tx.map(|tx| {
                        let tx_clone = tx.clone();
                        Box::new(move |chunk: String| {
                            let _ = tx_clone.send(chunk);
                        }) as Box<dyn Fn(String) + Send>
                    });

                    let result = crate::tools::execute::execute_command_safe(
                        command,
                        &resolved_workdir,
                        timeout,
                        chunk_cb,
                    );

                    if result.blocked {
                        return Err(result.stdout);
                    }

                    let mut output = String::new();
                    output.push_str(&result.stdout);

                    if let Some(code) = result.exit_code {
                        if code != 0 {
                            output.push_str(&format!("\n\n[Exit code: {}]", code));
                        }
                    }
                    if result.timed_out {
                        output.push_str(&format!("\n\n[Command timed out after {}s]", timeout));
                    }

                    Ok(output)
                }),
            }).await;

            // cron tool - manage scheduled tasks
            registry_clone.register(ToolDef {
                name: "cron".to_string(),
                description: "Manage cron jobs and scheduled tasks. You MUST write a valid 5-field cron expression yourself. Actions: list (list all), create (name + cron expression + payload), get (by id), update (by id), remove (by id), run (by id)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "create", "get", "update", "remove", "run"],
                            "description": "Action to perform"
                        },
                        "name": {
                            "type": "string",
                            "description": "Job name (required for create)"
                        },
                        "schedule": {
                            "type": "string",
                            "description": "CRON EXPRESSION (5 fields, e.g. '0 8 * * *' for daily 8am, '*/30 * * * *' for every 30min). You MUST generate the cron expression based on user's request. DO NOT use natural language!"
                        },
                        "payload": {
                            "type": "string",
                            "description": "Task prompt content to execute when the job fires (required for create)"
                        },
                        "id": {
                            "type": "string",
                            "description": "Job ID (required for get/update/remove/run)"
                        },
                        "session_id": {
                            "type": "string",
                            "description": "(Optional) Chat session ID for results delivery — if omitted, a dedicated cron session is created automatically"
                        }
                    },
                    "required": ["action"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("Missing 'action' parameter")?;
                    let store_arc = crate::cron::get_store();
                    let rt = tokio::runtime::Handle::current();

                    match action {
                        "list" => {
                            let guard = rt.block_on(async { store_arc.lock().await });
                            let jobs: Vec<crate::cron::CronJob> = guard.list().to_vec();
                            drop(guard);
                            let output: Vec<String> = jobs.iter().map(|j| {
                                format!("[{}] {} | schedule: {} | enabled: {} | runs: {}",
                                    j.id, j.name, j.schedule, j.enabled, j.run_count)
                            }).collect();
                            if output.is_empty() {
                                Ok("暂无定时任务".to_string())
                            } else {
                                Ok(output.join("\n"))
                            }
                        }
                        "get" => {
                            let id = args["id"].as_str().ok_or("Missing 'id' parameter")?;
                            let guard = rt.block_on(async { store_arc.lock().await });
                            let job = guard.get(id).cloned();
                            drop(guard);
                            match job {
                                Some(j) => Ok(format!(
                                    "ID: {}\n名称: {}\n调度: {}\n启用: {}\n状态: {}\n执行次数: {}\n上次执行: {:?}\n下次执行: {:?}\n负载: {}",
                                    j.id, j.name, j.schedule, j.enabled, j.status, j.run_count,
                                    j.last_run_at, j.next_run_at, j.payload
                                )),
                                None => Err(format!("定时任务 '{}' 未找到", id)),
                            }
                        }
                        "create" => {
                            let name = args["name"].as_str().ok_or("Missing 'name' parameter")?;
                            let schedule = args["schedule"].as_str().unwrap_or("0 * * * *");
                            let payload = args["payload"].as_str().unwrap_or("");

                            let id = uuid::Uuid::new_v4().to_string();
                            let now = chrono::Utc::now().to_rfc3339();
                            let next_run = crate::cron::compute_initial_next_run(schedule);

                            // 为定时任务创建专属会话（侧边栏显示为 ⏰ 任务名）
                            let cron_session_name = format!("⏰ {}", name);
                            let cron_session = rt.block_on(async {
                                crate::APP_STATE.read().await.session_store
                                    .create_session(&cron_session_name, None)
                                    .map_err(|e| format!("创建定时任务会话失败: {}", e))
                            })?;

                            let job = crate::cron::CronJob {
                                id: id.clone(),
                                name: name.to_string(),
                                schedule: schedule.to_string(),
                                enabled: true,
                                payload: payload.to_string(),
                                session_id: Some(cron_session.id.clone()),
                                created_at: now.clone(),
                                updated_at: now,
                                last_run_at: None,
                                next_run_at: Some(next_run),
                                status: "idle".to_string(),
                                run_count: 0,
                                last_error: None,
                            };

                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            guard.add(job);
                            drop(guard);
                            Ok(format!("定时任务 '{}' 已创建 (ID: {})", name, id))
                        }
                        "update" => {
                            let id = args["id"].as_str().ok_or("Missing 'id' parameter")?.to_string();
                            let name = args["name"].as_str().map(|s| s.to_string());
                            let schedule = args["schedule"].as_str().map(|s| s.to_string());
                            let payload = args["payload"].as_str().map(|s| s.to_string());
                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            let updated = guard.update(&id, |job| {
                                if let Some(ref n) = name { job.name = n.clone(); }
                                if let Some(ref sch) = schedule {
                                    job.schedule = sch.clone();
                                    job.next_run_at = Some(crate::cron::compute_initial_next_run(sch));
                                }
                                if let Some(ref p) = payload { job.payload = p.clone(); }
                            });
                            drop(guard);
                            if updated {
                                Ok(format!("定时任务已更新"))
                            } else {
                                Err(format!("定时任务 '{}' 未找到", id))
                            }
                        }
                        "remove" => {
                            let id = args["id"].as_str().ok_or("Missing 'id' parameter")?.to_string();
                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            let removed = guard.remove(&id);
                            drop(guard);
                            if removed {
                                let _ = crate::logging::delete_task_log(&id);
                                Ok(format!("定时任务已删除"))
                            } else {
                                Err(format!("定时任务 '{}' 未找到", id))
                            }
                        }
                        "run" => {
                            let id = args["id"].as_str().ok_or("Missing 'id' parameter")?.to_string();
                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            let updated = guard.update(&id, |job| {
                                job.last_run_at = Some(chrono::Utc::now().to_rfc3339());
                                job.run_count += 1;
                                tracing::info!("[Cron] 手动触发任务: {}", job.name);
                            });
                            drop(guard);
                            if updated {
                                Ok(format!("任务已手动触发"))
                            } else {
                                Err(format!("定时任务 '{}' 未找到", id))
                            }
                        }
                        _ => Err(format!("未知操作: {}", action)),
                    }
                }),
            }).await;

            // mcp_call_tool tool - proxy calls to MCP servers
            let mcp_store_for_tool = mcp_store.clone();
            registry_clone.register(ToolDef {
                name: "mcp_call_tool".to_string(),
                description: "Call a tool on an MCP (Model Context Protocol) server. Params: server_name (required), tool_name (required), arguments (optional JSON object)".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "server_name": {
                            "type": "string",
                            "description": "Name of the MCP server to call"
                        },
                        "tool_name": {
                            "type": "string",
                            "description": "Name of the tool to invoke on the MCP server"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "JSON arguments to pass to the tool (optional)"
                        }
                    },
                    "required": ["server_name", "tool_name"]
                }),
                handler: std::sync::Arc::new(move |args: serde_json::Value, _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>| -> Result<String, String> {
                    let server_name = args["server_name"].as_str().ok_or("Missing 'server_name' parameter")?.to_string();
                    let tool_name = args["tool_name"].as_str().ok_or("Missing 'tool_name' parameter")?.to_string();
                    let tool_args = args.get("arguments").cloned().unwrap_or(serde_json::Value::Null);
                    let store_clone = mcp_store_for_tool.clone();

                    let store_guard = std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new()
                            .map_err(|e| format!("Runtime error: {}", e))?;

                        rt.block_on(async move {
                            let server = {
                                let guard = store_clone.lock().await;
                                guard.get(&server_name).cloned()
                            };
                            let server = match server {
                                Some(s) => s,
                                None => return Err(format!("MCP 服务器 '{}' 未找到", server_name)),
                            };
                            if !server.enabled {
                                return Err(format!("MCP 服务器 '{}' 已禁用", server_name));
                            }
                            crate::mcp::call_tool(&server, &tool_name, tool_args).await
                        })
                    }).join()
                    .map_err(|_| "MCP tool call thread crashed".to_string())??;

                    Ok(store_guard)
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
        return format!("No search results for '{}'", query);
    }

    let mut output = Vec::new();
    let source_tag = if source.is_empty() { String::new() } else { format!(" - Source: {}", source) };
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

/// Apply a unified diff patch to files
fn apply_unified_diff(diff: &str, _args: &serde_json::Value) -> Result<Vec<String>, String> {
    let mut summary: Vec<String> = Vec::new();
    let mut current_file: Option<String> = None;
    let mut hunks: Vec<(usize, Vec<String>)> = Vec::new(); // (start_line, hunk_lines)

    // Parse unified diff format
    for line in diff.lines() {
        if line.starts_with("--- ") {
            continue; // Skip original file header
        }
        if line.starts_with("+++ ") {
            // New file header -> finalize current file
            if let Some(ref file) = current_file {
                if !hunks.is_empty() {
                    apply_hunks_to_file(file, &hunks)?;
                    summary.push(format!("  Patched: {}", file));
                }
            }
            let path = &line[4..].trim();
            current_file = Some(path.to_string());
            hunks.clear();
            continue;
        }
        if line.starts_with("@@ ") {
            // Parse hunk header: @@ -start,count +start,count @@
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Some(new_start) = parts.get(1).and_then(|s| s.split(',').next()) {
                    let start_line = new_start.trim_start_matches('+').parse::<usize>().unwrap_or(1);
                    hunks.push((start_line, Vec::new()));
                }
            }
            continue;
        }

        if let Some((_, ref mut lines)) = hunks.last_mut() {
            lines.push(line.to_string());
        }
    }

    // Apply last file
    if let Some(ref file) = current_file {
        if !hunks.is_empty() {
            apply_hunks_to_file(file, &hunks)?;
            summary.push(format!("  Patched: {}", file));
        }
    }

    if summary.is_empty() {
        return Err("No patch hunks to apply - check diff format".to_string());
    }

    Ok(summary)
}

/// Apply parsed hunks to a single file
fn apply_hunks_to_file(file: &str, hunks: &[(usize, Vec<String>)]) -> Result<(), String> {
    let path = std::path::Path::new(file);
    let content = if path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read '{}': {}", file, e))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    // Apply hunks in reverse order to preserve line numbers
    for (hunk_start, hunk_lines) in hunks.iter().rev() {
        let mut insertions: Vec<(usize, String)> = Vec::new();
        let mut deletions: Vec<usize> = Vec::new();
        let mut current_line = *hunk_start - 1; // 0-based

        for hunk_line in hunk_lines {
            if hunk_line.starts_with("+") {
                // Insertion
                let text = &hunk_line[1..];
                // If next line in hunk is a context line, insert before that
                insertions.push((current_line.min(lines.len()), text.to_string()));
            } else if hunk_line.starts_with("-") {
                // Deletion
                if current_line < lines.len() {
                    deletions.push(current_line);
                }
                // Don't advance line number for deletions
            } else if hunk_line.starts_with(" ") {
                // Context - advance line number
                current_line = current_line.saturating_add(1);
            }
        }

        // Apply deletions (reverse order to preserve indices)
        for &dl in deletions.iter().rev() {
            if dl < lines.len() {
                lines.remove(dl);
            }
        }

        // Apply insertions (reverse order to preserve indices, then fix order)
        let mut offset = 0i32;
        for (pos, text) in &insertions {
            let adj_pos = ((*pos as i32) + offset).max(0) as usize;
            // Check if we should replace or insert
            if adj_pos <= lines.len() {
                lines.insert(adj_pos, text.clone());
                offset += 1;
            }
        }
    }

    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    std::fs::write(path, lines.join("\n"))
        .map_err(|e| format!("Failed to write '{}': {}", file, e))?;

    Ok(())
}