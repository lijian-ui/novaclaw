use crate::tools::registry::{ToolDef, ToolRegistry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

/// 每个待办项
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoItem {
    content: String,
    status: String, // "pending" | "in_progress" | "completed"
    #[serde(default = "default_priority")]
    priority: String, // "high" | "medium" | "low"
}

fn default_priority() -> String {
    "medium".to_string()
}

/// 持久化结构
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoList {
    items: Vec<TodoItem>,
    updated_at: String,
}

/// 获取 todo 文件路径（按 session_id 隔离）
fn todo_path(session_id: &str) -> PathBuf {
    crate::config::get_base_dir()
        .join("todo")
        .join(format!("{}.json", session_id))
}

/// 加载当前 todo 列表
fn load_todos(session_id: &str) -> TodoList {
    let path = todo_path(session_id);
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(TodoList {
                items: vec![],
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
    } else {
        TodoList {
            items: vec![],
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// 保存 todo 列表
fn save_todos(session_id: &str, list: &TodoList) -> Result<(), String> {
    let path = todo_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 todo 目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化 todo 失败: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("写入 todo 文件失败: {}", e))?;
    Ok(())
}

/// 将 TodoList 格式化为 LLM 可读的文本
fn format_todo_list(list: &TodoList) -> String {
    if list.items.is_empty() {
        return "当前没有待办任务。".to_string();
    }
    let mut lines = vec![format!("📋 待办列表 (共 {} 项):", list.items.len())];
    for (i, item) in list.items.iter().enumerate() {
        let status_icon = match item.status.as_str() {
            "in_progress" => "🔄",
            "completed" => "✅",
            _ => "⬜",
        };
        let priority_label = match item.priority.as_str() {
            "high" => " [高]",
            "low" => " [低]",
            _ => "",
        };
        lines.push(format!("{}. {}{} {}", i + 1, status_icon, priority_label, item.content,));
    }
    // 统计
    let total = list.items.len();
    let done = list.items.iter().filter(|i| i.status == "completed").count();
    let in_progress = list.items.iter().filter(|i| i.status == "in_progress").count();
    lines.push(format!("\n进度: {}/{} 已完成", done, total));
    if in_progress > 0 {
        lines.push(format!("进行中: {} 项", in_progress));
    }
    lines.join("\n")
}

/// 注册 todo 相关工具: todo_write, todo_list
pub async fn register(registry: &ToolRegistry) {
    // ── todo_write: set 语义，一次替换整个列表 ──
    registry
        .register(ToolDef {
            name: "todo_write".to_string(),
            description:
                "Write the full todo task list (replace mode). One call sets the entire list, completely replacing the previous one.\n\nEach task can have a status: pending, in_progress, completed\nOnly one task can be in_progress at a time.\nTasks can have priority: high, medium, low.\n\nUse cases:\n- Plan before starting complex tasks\n- Update task status during execution\n- Mark tasks as completed when done".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "List of tasks (replaces the entire list)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {
                                    "type": "string",
                                    "description": "Task description"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"],
                                    "description": "Task status: pending, in_progress, completed"
                                },
                                "priority": {
                                    "type": "string",
                                    "enum": ["high", "medium", "low"],
                                    "description": "Task priority (default: medium)"
                                }
                            },
                            "required": ["content", "status"]
                        }
                    }
                },
                "required": ["items"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let items_val = args
                        .get("items")
                        .and_then(|v| v.as_array())
                        .ok_or("Missing 'items' parameter - provide an array of tasks")?;

                    // 反序列化并验证
                    let items: Vec<TodoItem> =
                        serde_json::from_value(serde_json::Value::Array(items_val.clone()))
                            .map_err(|e| format!("参数格式错误: {}", e))?;

                    if items.is_empty() {
                        return Err("任务列表不能为空".to_string());
                    }

                    // 验证：最多一个 in_progress
                    let in_progress_count = items.iter().filter(|i| i.status == "in_progress").count();
                    if in_progress_count > 1 {
                        return Err("一次只能有一个进行中的任务（in_progress）".to_string());
                    }

                    // 获取 session_id
                    let session_id = args["_session_id"].as_str().unwrap_or("default");
                    let list = TodoList {
                        items,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    };
                    save_todos(session_id, &list)?;

                    Ok(format_todo_list(&list))
                },
            ),
        })
        .await;

    // ── todo_list: 查看当前待办列表 ──
    registry
        .register(ToolDef {
            name: "todo_list".to_string(),
            description:
                "View the current session's todo task list. Returns a formatted list with status and progress."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let session_id = args["_session_id"].as_str().unwrap_or("default");
                    let list = load_todos(session_id);
                    Ok(format_todo_list(&list))
                },
            ),
        })
        .await;
}
