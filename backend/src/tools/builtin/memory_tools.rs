use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册记忆/知识/技能相关工具: memory, session_search, skill_view, todo
pub async fn register(
    registry: &ToolRegistry,
    memory_store: &std::sync::Arc<crate::memory::store::MemoryStore>,
    session_store: &std::sync::Arc<crate::storage::SessionStore>,
    skills_loader: &std::sync::Arc<crate::skills::loader::SkillsLoader>,
) {
    // memory tool
    let memory_store_for_memory = memory_store.clone();
    registry
        .register(ToolDef {
                        name: "memory".to_string(),
            display_name: "持久记忆".to_string(),
            description: "Save and search persistent facts across sessions. Actions: add (save fact), search (find by keyword), replace (update, use 'old | new' format), remove (delete). Not for temporary data — use session_search instead."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["add", "search", "replace", "remove"],
                        "description": "Action: add=save fact, search=find memories, replace=update, remove=delete"
                    },
                    "content": {
                        "type": "string",
                        "description": "The fact text (for add/replace/remove). Use declarative statements like 'User prefers Golang'"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search keyword (used for search action)"
                    },
                    "category": {
                        "type": "string",
                        "description": "Optional category label (e.g. 'preference', 'convention', 'environment')"
                    }
                },
                "required": ["action"]
            }),
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let action = args["action"]
                        .as_str()
                        .ok_or("Missing 'action' parameter. Valid: add, search, replace, remove")?;
                    match action {
                        "add" => {
                            let content = args["content"]
                                .as_str()
                                .ok_or("Missing 'content' parameter for add")?;
                            let category = args
                                .get("category")
                                .and_then(|v| v.as_str())
                                .unwrap_or("general");
                            memory_store_for_memory
                                .add_memory(content, category)
                                .map_err(|e| format!("添加失败: {}", e))?;
                            Ok(format!("已保存: \"{}\"", content))
                        }
                        "search" => {
                            let query = args["query"].as_str().unwrap_or("");
                            let results = memory_store_for_memory.search_memories(query);
                            if results.is_empty() {
                                Ok("未找到相关记忆".to_string())
                            } else {
                                Ok(format!(
                                    "找到 {} 条相关记忆:\n\n{}",
                                    results.len(),
                                    results.join("\n---\n")
                                ))
                            }
                        }
                        "replace" => {
                            let content = args["content"].as_str().ok_or(
                                "Missing 'content' parameter. Format: 'old text | new text'",
                            )?;
                            let parts: Vec<&str> = content.splitn(2, '|').collect();
                            if parts.len() != 2 {
                                return Err("replace 需要 '旧内容 | 新内容' 格式，用 | 分隔"
                                    .to_string());
                            }
                            memory_store_for_memory.replace_memory(parts[0], parts[1])
                        }
                        "remove" => {
                            let content = args["content"]
                                .as_str()
                                .ok_or("Missing 'content' parameter for remove")?;
                            memory_store_for_memory
                                .remove_memory(content)
                                .map_err(|e| format!("删除失败: {}", e))?;
                            Ok(format!("已删除: \"{}\"", content))
                        }
                        "list" => {
                            let all = memory_store_for_memory.list_memories();
                            if all.is_empty() {
                                Ok("暂无记忆".to_string())
                            } else {
                                Ok(format!("所有记忆:\n\n{}", all))
                            }
                        }
                        _ => Err(format!(
                            "不支持的操作 '{}'。可用: add, search, replace, remove, list",
                            action
                        )),
                    }
                },
            ),
        })
        .await;

    // session_search tool
    let session_store_for_search = session_store.clone();
    registry
        .register(ToolDef {
                        name: "session_search".to_string(),
            display_name: "会话搜索".to_string(),
            description:
                "Search the current session history for temporary info like task progress or past decisions. Use instead of memory/search for session-specific data. Params: query (required), limit (optional, default 5)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search keyword or phrase"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default 5)"
                    }
                },
                "required": ["query"]
            }),
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let query = args["query"].as_str().ok_or("Missing 'query' parameter")?;
                    let limit = args
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(5) as usize;
                    let query_lower = query.to_lowercase();

                    let session_id = args["_session_id"].as_str().unwrap_or("");
                    if session_id.is_empty() {
                        return Err("无法获取当前会话 ID".to_string());
                    }

                    let messages = session_store_for_search
                        .get_messages(session_id)
                        .map_err(|e| format!("读取会话历史失败: {}", e))?;

                    let mut results: Vec<String> = Vec::new();
                    for msg in messages.iter().rev() {
                        if msg.content.to_lowercase().contains(&query_lower) {
                            let role_icon = match msg.role.as_str() {
                                "user" => "👤",
                                "assistant" => "🤖",
                                "tool" => "🔧",
                                _ => "💬",
                            };
                            let preview: String = msg.content.chars().take(200).collect();
                            let suffix = if msg.content.len() > 200 { "..." } else { "" };
                            results.push(format!("{} [{}] {}{}", role_icon, msg.role, preview, suffix));
                            if results.len() >= limit {
                                break;
                            }
                        }
                    }

                    if results.is_empty() {
                        Ok(format!(
                            "在会话历史中未找到与 '{}' 相关的消息",
                            query
                        ))
                    } else {
                        Ok(format!(
                            "找到 {} 条相关历史消息:\n\n{}",
                            results.len(),
                            results.join("\n\n")
                        ))
                    }
                },
            ),
        })
        .await;

    // skill_view tool
    let skills_loader_for_view = skills_loader.clone();
    registry
        .register(ToolDef {
                        name: "skill_view".to_string(),
            display_name: "查看技能".to_string(),
            description:
                "Load a skill's full instructions and resources. Skills contain specialized knowledge — API endpoints, commands, and proven workflows. ALWAYS load a relevant skill before attempting a task with generic tools. Call first to get the instructions and a linked_files listing; then call again with file_path to load scripts, templates, or references. Available skills are listed under '## Skills (mandatory)' in the system prompt."
                    .to_string(),
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
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let name = args["name"]
                        .as_str()
                        .ok_or("Missing 'name' parameter - provide the skill name")?;
                    tracing::info!("[Skill] skill_view 加载技能: {}", name);
                    match skills_loader_for_view.get_skill(name) {
                        Some(skill) => {
                            let raw = &skill.source_path;
                            let normalized_dir = if cfg!(target_os = "windows") {
                                raw.replace('/', "\\")
                            } else {
                                raw.replace('\\', "/")
                            };
                            let content = skill
                                .content
                                .replace("{SKILL_DIR}", &normalized_dir)
                                .replace("${SKILL_DIR}", &normalized_dir)
                                .replace("${HERMES_SKILL_DIR}", &normalized_dir);

                            // 扫描技能目录下的子目录作为 linked_files
                            let skill_path = std::path::Path::new(&normalized_dir);
                            let mut linked_files = serde_json::Map::new();
                            if let Ok(entries) = std::fs::read_dir(skill_path) {
                                for entry in entries.flatten() {
                                    if let Ok(ftype) = entry.file_type() {
                                        if ftype.is_dir() {
                                            let dir_name = entry.file_name().to_string_lossy().to_string();
                                            if let Ok(files) = std::fs::read_dir(entry.path()) {
                                                let file_list: Vec<String> = files
                                                    .flatten()
                                                    .filter(|f| f.file_type().map(|t| t.is_file()).unwrap_or(false))
                                                    .filter_map(|f| {
                                                        f.file_name().to_str().map(|s| format!("{}/{}", dir_name, s))
                                                    })
                                                    .collect();
                                                if !file_list.is_empty() {
                                                    linked_files.insert(dir_name, serde_json::json!(file_list));
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let mut result = serde_json::json!({
                                "name": skill.name,
                                "description": skill.description,
                                "version": skill.version,
                                "content": content,
                                "skill_dir": normalized_dir,
                            });
                            if !linked_files.is_empty() {
                                result["linked_files"] = serde_json::Value::Object(linked_files);
                                result["usage_hint"] = serde_json::Value::String(
                                    "To view linked files, call skill_view with file_path, e.g.: skill_view(name, file_path='scripts/run.sh')".to_string()
                                );
                            }
                            Ok(result.to_string())
                        }
                        None => {
                            let available: Vec<String> = skills_loader_for_view
                                .list_skills()
                                .iter()
                                .map(|s| s.name.clone())
                                .collect();
                            Err(format!(
                                "Skill '{}' not found. Available skills: {}",
                                name,
                                available.join(", ")
                            ))
                        }
                    }
                },
            ),
        })
        .await;

}

