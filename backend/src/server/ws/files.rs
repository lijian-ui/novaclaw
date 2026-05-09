use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

/// 被监视的文件条目：路径 + 上次修改时间
struct WatchEntry {
    path: String,
    last_modified: std::time::SystemTime,
}

/// 全局监视状态
static WATCHED_FILES: once_cell::sync::Lazy<Arc<Mutex<HashMap<String, WatchEntry>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// 保存文件内容
fn save_file_content(path: &str, content: &str, ws_str: &str) -> Result<(), String> {
    let target = Path::new(path);
    let target_str = target.to_string_lossy().to_string();

    // 路径穿越防护
    if !target_str.starts_with(ws_str) {
        return Err(format!("路径不在工作区内: {} (workspace: {})", target_str, ws_str));
    }

    // 写入前确保父目录存在
    if let Some(parent) = target.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建父目录失败: {}", e))?;
        }
    }

    tracing::info!("WebSocket 写入文件: {}", path);
    std::fs::write(target, content)
        .map_err(|e| format!("写入文件失败: {}", e))
}

/// 读取文件内容（带路径穿越防护，支持非 UTF-8 降级）
fn read_file_content(path: &str) -> Result<String, String> {
    let target = Path::new(path);
    if !target.exists() {
        return Err("文件不存在".to_string());
    }
    if target.is_dir() {
        return Err("路径是目录".to_string());
    }
    let bytes = std::fs::read(target).map_err(|e| format!("读取文件失败: {}", e))?;
    // 使用 lossy 转换，非 UTF-8 字节替换为 ?
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// 删除文件或目录
fn delete_path(path: &str) -> Result<(), String> {
    let target = Path::new(path);
    if !target.exists() {
        return Err("路径不存在".to_string());
    }
    if target.is_dir() {
        std::fs::remove_dir_all(target).map_err(|e| format!("删除目录失败: {}", e))
    } else {
        std::fs::remove_file(target).map_err(|e| format!("删除文件失败: {}", e))
    }
}

/// 重命名文件或目录
fn rename_path(old_path: &str, new_path: &str) -> Result<(), String> {
    std::fs::rename(old_path, new_path).map_err(|e| format!("重命名失败: {}", e))
}

/// 复制文件或目录（递归）
fn copy_path(source: &str, dest: &str) -> Result<(), String> {
    let src = Path::new(source);
    let dst = Path::new(dest);
    if src.is_dir() {
        copy_dir_recursive(src, dst).map_err(|e| format!("复制目录失败: {}", e))
    } else {
        std::fs::copy(src, dst).map_err(|e| format!("复制文件失败: {}", e))?;
        Ok(())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
fn list_directory(path: &str) -> Result<Vec<serde_json::Value>, String> {
    let dir = Path::new(path);
    if !dir.exists() || !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    let rd = std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;

    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let full_path = entry.path();
        let metadata = entry.metadata().ok();
        let is_dir = full_path.is_dir();
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.to_rfc3339()
            })
            .unwrap_or_default();
        let extension = full_path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        entries.push(serde_json::json!({
            "name": name,
            "path": full_path.to_string_lossy(),
            "is_dir": is_dir,
            "size": size,
            "modified": modified,
            "extension": extension,
        }));
    }

    // 排序：目录在前
    entries.sort_by(|a, b| {
        let a_dir = a["is_dir"].as_bool().unwrap_or(false);
        let b_dir = b["is_dir"].as_bool().unwrap_or(false);
        if a_dir != b_dir {
            b_dir.cmp(&a_dir)
        } else {
            a["name"].as_str().unwrap_or("").to_lowercase()
                .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
        }
    });

    Ok(entries)
}

/// 文件变更监控任务（轮询模式）
async fn watch_files_task(
    ws_sender: Arc<Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
) {
    let mut tick = interval(Duration::from_secs(1));

    loop {
        tick.tick().await;
        let watched = WATCHED_FILES.lock().await;
        if watched.is_empty() {
            continue;
        }

        let mut changed = Vec::new();

        for (id, entry) in watched.iter() {
            let path = Path::new(&entry.path);
            if !path.exists() {
                changed.push((id.clone(), "deleted".to_string(), String::new()));
                continue;
            }
            if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified != entry.last_modified {
                        let content = std::fs::read_to_string(path).unwrap_or_default();
                        changed.push((id.clone(), "changed".to_string(), content));
                    }
                }
            }
        }

        drop(watched);

        for (id, change_type, content) in &changed {
            // 更新文件的修改时间
            if let Ok(meta) = Path::new(&id).metadata() {
                if let Ok(modified) = meta.modified() {
                    let mut w = WATCHED_FILES.lock().await;
                    if let Some(entry) = w.get_mut(id.as_str()) {
                        entry.last_modified = modified;
                    }
                }
            }
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({
                        "type": "file_changed",
                        "path": id,
                        "change": change_type,
                        "content": content,
                    })
                    .to_string(),
                ))
                .await;
        }
    }
}

/// WebSocket 文件操作处理器
async fn handle_file_socket(socket: WebSocket) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    // 获取工作区路径
    let state = crate::APP_STATE.read().await;
    let ws_str = state.config.workspace_dir().to_string_lossy().to_string();
    drop(state);

    // 启动文件监控任务
    let watcher = Arc::clone(&ws_sender);
    tokio::spawn(async move { watch_files_task(watcher).await });

    while let Some(msg) = ws_receiver.next().await {
        let msg_text = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        let parsed: serde_json::Value = match serde_json::from_str(&msg_text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let cmd_type = parsed["type"].as_str().unwrap_or("").to_string();
        let cmd_path = parsed["path"].as_str().unwrap_or("").to_string();

        // 安全检查：路径必须在 workspace 内
        if !cmd_path.is_empty() && !cmd_path.starts_with(&ws_str) {
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"type":"error","message":"路径不在工作区内"}).to_string(),
                ))
                .await;
            continue;
        }

        match cmd_type.as_str() {
            "read" => {
                let result = read_file_content(&cmd_path);
                let mut sender = ws_sender.lock().await;
                match result {
                    Ok(content) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "read_result",
                                    "path": cmd_path,
                                    "content": content,
                                    "success": true,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                    Err(e) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "read_result",
                                    "path": cmd_path,
                                    "success": false,
                                    "message": e,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
            }
            "write" => {
                let content = parsed["content"].as_str().unwrap_or("");
                let result = save_file_content(&cmd_path, content, &ws_str);
                let mut sender = ws_sender.lock().await;
                match result {
                    Ok(_) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "write_result",
                                    "path": cmd_path,
                                    "success": true,
                                })
                                .to_string(),
                            ))
                            .await;
                        // 通知前端刷新父目录
                        let parent = Path::new(&cmd_path).parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| ws_str.clone());
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "dir_changed",
                                    "path": parent,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                    Err(e) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "write_result",
                                    "path": cmd_path,
                                    "success": false,
                                    "message": e,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
            }
            "list" => {
                let list_path = if cmd_path.is_empty() { ws_str.clone() } else { cmd_path.clone() };
                tracing::info!("WebSocket 列出目录: {}", list_path);
                let result = list_directory(&list_path);
                let mut sender = ws_sender.lock().await;
                match result {
                    Ok(entries) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "list_result",
                                    "path": cmd_path,
                                    "entries": entries,
                                    "success": true,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                    Err(e) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "list_result",
                                    "path": cmd_path,
                                    "success": false,
                                    "message": e,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
            }
            "watch" => {
                let path = Path::new(&cmd_path);
                let last_modified = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or_else(std::time::SystemTime::now);

                let mut watched = WATCHED_FILES.lock().await;
                watched.insert(cmd_path.clone(), WatchEntry {
                    path: cmd_path.clone(),
                    last_modified,
                });

                let mut sender = ws_sender.lock().await;
                let _ = sender
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "watch_started",
                            "path": cmd_path,
                        })
                        .to_string(),
                    ))
                    .await;
            }
            "unwatch" => {
                let mut watched = WATCHED_FILES.lock().await;
                watched.remove(&cmd_path);

                let mut sender = ws_sender.lock().await;
                let _ = sender
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "watch_stopped",
                            "path": cmd_path,
                        })
                        .to_string(),
                    ))
                    .await;
            }
            "get_workspace" => {
                let mut sender = ws_sender.lock().await;
                let _ = sender
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "workspace_info",
                            "workspace": ws_str,
                        })
                        .to_string(),
                    ))
                    .await;
            }
            "delete" => {
                let result = delete_path(&cmd_path);
                let mut sender = ws_sender.lock().await;
                match result {
                    Ok(_) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "delete_result",
                                    "path": cmd_path,
                                    "success": true,
                                })
                                .to_string(),
                            ))
                            .await;
                        // 通知前端刷新父目录
                        let parent = Path::new(&cmd_path).parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| ws_str.clone());
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "dir_changed",
                                    "path": parent,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                    Err(e) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "delete_result",
                                    "path": cmd_path,
                                    "success": false,
                                    "message": e,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
            }
            "rename" => {
                let new_path = parsed["new_path"].as_str().unwrap_or("");
                let result = rename_path(&cmd_path, new_path);
                let mut sender = ws_sender.lock().await;
                match result {
                    Ok(_) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "rename_result",
                                    "path": cmd_path,
                                    "new_path": new_path,
                                    "success": true,
                                })
                                .to_string(),
                            ))
                            .await;
                        // 通知前端刷新源父目录和目标父目录
                        let src_parent = Path::new(&cmd_path).parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| ws_str.clone());
                        let dst_parent = Path::new(new_path).parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| ws_str.clone());
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "dir_changed",
                                    "path": src_parent,
                                })
                                .to_string(),
                            ))
                            .await;
                        if dst_parent != src_parent {
                            let _ = sender
                                .send(Message::Text(
                                    serde_json::json!({
                                        "type": "dir_changed",
                                        "path": dst_parent,
                                    })
                                    .to_string(),
                                ))
                                .await;
                        }
                    }
                    Err(e) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "rename_result",
                                    "path": cmd_path,
                                    "new_path": new_path,
                                    "success": false,
                                    "message": e,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
            }
            "copy" => {
                let dest = parsed["dest"].as_str().unwrap_or("");
                let result = copy_path(&cmd_path, dest);
                let mut sender = ws_sender.lock().await;
                match result {
                    Ok(_) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "copy_result",
                                    "path": cmd_path,
                                    "dest": dest,
                                    "success": true,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                    Err(e) => {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "copy_result",
                                    "path": cmd_path,
                                    "dest": dest,
                                    "success": false,
                                    "message": e,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
            }
            _ => {}
        }
    }

    // 连接断开，清理监视
    let mut watched = WATCHED_FILES.lock().await;
    watched.clear();
}

async fn ws_files_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_file_socket(socket))
}

pub fn routes() -> Router {
    Router::new().route("/files", get(ws_files_handler))
}
