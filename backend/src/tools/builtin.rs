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
pub fn register_all(registry: &mut ToolRegistry) {
    let rt = tokio::runtime::Handle::current();
    let registry_clone = registry.clone();

    // 注册时不需要阻塞等待，使用 spawn_blocking 方式
    // 由于初始化在 tokio 上下文中，可以直接 block_on
    std::thread::spawn(move || {
        rt.block_on(async move {
            // read_file 工具
            registry_clone.register(ToolDef {
                name: "read_file".to_string(),
                description: "读取文件内容".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "文件路径（相对路径会被解析到工作目录，绝对路径直接使用）"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "起始行号"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "读取行数上限"
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

            // write_file 工具
            registry_clone.register(ToolDef {
                name: "write_file".to_string(),
                description: "写入文件内容".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "文件路径（相对路径会被解析到工作目录，绝对路径直接使用）"
                        },
                        "content": {
                            "type": "string",
                            "description": "要写入的内容"
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

            // edit_file 工具
            registry_clone.register(ToolDef {
                name: "edit_file".to_string(),
                description: "编辑文件（查找替换）".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "文件路径（相对路径会被解析到工作目录，绝对路径直接使用）"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "要替换的旧内容"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "替换后的新内容"
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

            // glob 文件搜索工具
            registry_clone.register(ToolDef {
                name: "glob".to_string(),
                description: "按 glob 模式搜索文件".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "glob 模式，如 **/*.rs"
                        },
                        "path": {
                            "type": "string",
                            "description": "搜索根目录（相对路径会被解析到工作目录，绝对路径直接使用），默认工作目录"
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

            // grep 内容搜索工具
            registry_clone.register(ToolDef {
                name: "grep".to_string(),
                description: "在文件中搜索指定文本模式".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "正则表达式模式"
                        },
                        "path": {
                            "type": "string",
                            "description": "搜索目录（相对路径会被解析到工作目录，绝对路径直接使用），默认工作目录"
                        },
                        "include": {
                            "type": "string",
                            "description": "包含的文件模式，如 *.rs"
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

            // memory 工具
            registry_clone.register(ToolDef {
                name: "memory".to_string(),
                description: "持久化记忆管理：add(添加) / query(查询) / remove(删除)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["add", "query", "remove"],
                            "description": "操作类型"
                        },
                        "content": {
                            "type": "string",
                            "description": "记忆内容（add/remove 时使用）"
                        },
                        "query": {
                            "type": "string",
                            "description": "查询关键字（query 时使用）"
                        }
                    },
                    "required": ["action"]
                }),
                handler: std::sync::Arc::new(|args: serde_json::Value| -> Result<String, String> {
                    use crate::APP_STATE;

                    let state = APP_STATE.blocking_read();
                    let action = args["action"].as_str().ok_or("缺少 action 参数")?;

                    match action {
                        "add" => {
                            let content = args["content"].as_str().ok_or("缺少 content 参数")?;
                            let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("general");
                            state.memory_store.add_memory(content, category)
                                .map_err(|e| format!("记忆添加失败: {}", e))?;
                            Ok("记忆已保存".to_string())
                        }
                        "query" => {
                            let query = args["query"].as_str().unwrap_or("");
                            let memories = state.memory_store.query_memories(query)
                                .map_err(|e| format!("记忆查询失败: {}", e))?;
                            if memories.is_empty() {
                                Ok("未找到相关记忆".to_string())
                            } else {
                                Ok(memories.join("\n---\n"))
                            }
                        }
                        "remove" => {
                            let content = args["content"].as_str().ok_or("缺少 content 参数")?;
                            state.memory_store.remove_memory(content)
                                .map_err(|e| format!("记忆删除失败: {}", e))?;
                            Ok("记忆已删除".to_string())
                        }
                        _ => Err(format!("未知操作: {}", action)),
                    }
                }),
            }).await;

            // session_search 工具
            registry_clone.register(ToolDef {
                name: "session_search".to_string(),
                description: "搜索历史会话消息".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索关键字"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "返回数量上限"
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

            // web_search 工具 - 使用 DuckDuckGo API
            let web_search_client = std::sync::Arc::new(reqwest::Client::new());
            registry_clone.register(ToolDef {
                name: "web_search".to_string(),
                description: "通过 DuckDuckGo 搜索引擎搜索网络信息".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索查询词"
                        },
                        "count": {
                            "type": "integer",
                            "description": "返回结果数量（默认5，最大10）"
                        },
                        "lang": {
                            "type": "string",
                            "description": "语言限制（zh/en，默认自动检测）"
                        }
                    },
                    "required": ["query"]
                }),
                handler: std::sync::Arc::new(move |args: serde_json::Value| -> Result<String, String> {
                    let query = args["query"].as_str().ok_or("缺少 query 参数")?;
                    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                    let lang = args.get("lang").and_then(|v| v.as_str()).unwrap_or("");

                    // 查询预处理
                    let processed_query = preprocess_query(query, None);
                    tracing::info!("Web搜索查询（预处理后）: {}", processed_query);

                    // 构建 DuckDuckGo API 请求
                    let mut url = format!(
                        "https://api.duckduckgo.com/?q={}&format=json&no_redirect=1&no_html=1",
                        urlencoding::encode(&processed_query)
                    );
                    
                    // 添加语言参数
                    if !lang.is_empty() {
                        url.push_str(&format!("&kl={}", lang));
                    }

                    // 使用 tokio 运行时执行异步请求
                    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("创建运行时失败: {}", e))?;
                    let client_clone = web_search_client.clone();
                    
                    let result: Result<serde_json::Value, String> = rt.block_on(async move {
                        let response = client_clone.get(&url)
                            .header("User-Agent", "NovaClaw/1.0 (https://novaclaw.ai)")
                            .send()
                            .await
                            .map_err(|e| format!("网络请求失败: {}", e))?;

                        response.json().await.map_err(|e| format!("解析响应失败: {}", e))
                    });

                    let raw_results = result?;

                    // 解析并排序结果
                    let results = parse_and_rank(&raw_results, query);

                    // 格式化输出
                    if results.is_empty() {
                        Ok(format!("未找到与 '{}' 相关的搜索结果", query))
                    } else {
                        let mut output = Vec::new();
                        output.push(format!("🔍 搜索结果（共 {} 条）:", results.len()));
                        output.push("---".to_string());
                        
                        for (idx, result) in results.iter().take(count).enumerate() {
                            output.push(format!("{}. **{}**", idx + 1, result.title));
                            output.push(format!("   📄 {}", result.url));
                            if !result.snippet.is_empty() {
                                let snippet = if result.snippet.len() > 200 {
                                    &result.snippet[..200]
                                } else {
                                    &result.snippet
                                };
                                output.push(format!("   💡 {}", snippet));
                            }
                            output.push("".to_string());
                        }
                        
                        Ok(output.join("\n"))
                    }
                }),
            }).await;

            // todo 工具
            registry_clone.register(ToolDef {
                name: "todo".to_string(),
                description: "任务管理：add(添加) / list(列表) / done(完成) / remove(删除)".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["add", "list", "done", "remove"],
                            "description": "操作类型"
                        },
                        "title": {
                            "type": "string",
                            "description": "任务标题"
                        },
                        "id": {
                            "type": "string",
                            "description": "任务ID（done/remove 时使用）"
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
