use axum::{
    extract::Query,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use crate::APP_STATE;

#[derive(Deserialize)]
struct CreateSessionReq {
    name: String,
    #[serde(default)]
    model: Option<String>,
}

/// 列出所有会话
async fn list_sessions() -> Json<serde_json::Value> {
    tracing::debug!("list_sessions 被调用");
    let state = APP_STATE.read().await;
    match state.session_store.list_sessions() {
        Ok(sessions) => {
            tracing::debug!("list_sessions 成功: {} 个会话", sessions.len());
            Json(serde_json::json!({ "success": true, "data": sessions }))
        }
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

/// 删除会话（通过查询参数 ?session_id=xxx）
async fn delete_session(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let session_id = match params.get("session_id") {
        Some(id) => id,
        None => return Json(serde_json::json!({"success": false, "message": "缺少 session_id 参数"})),
    };
    tracing::info!("delete_session: session_id={}", session_id);
    let state = APP_STATE.read().await;
    match state.session_store.delete_session(session_id) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

/// 创建新会话
async fn create_session(Json(req): Json<CreateSessionReq>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.session_store.create_session(&req.name, req.model.as_deref()) {
        Ok(session) => Json(serde_json::json!({ "success": true, "data": session })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

/// 获取会话消息（通过查询参数 ?session_id=xxx&limit=50）
async fn get_session_messages(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let session_id = match params.get("session_id") {
        Some(id) => id,
        None => return Json(serde_json::json!({"success": false, "message": "缺少 session_id 参数"})),
    };
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    tracing::info!("get_session_messages: session_id={}", session_id);
    let state = APP_STATE.read().await;
    // 检查消息文件路径
    let msg_path = state.session_store.messages_path_for_debug(session_id);
    tracing::info!("消息文件路径: {:?}", msg_path);
    tracing::info!("消息文件是否存在: {}", msg_path.exists());
    if msg_path.exists() {
        match std::fs::read_to_string(&msg_path) {
            Ok(content) => tracing::info!("消息文件内容(前200字符): {}", &content.chars().take(200).collect::<String>()),
            Err(e) => tracing::error!("读取消息文件失败: {}", e),
        }
    }
    match state.session_store.get_messages(session_id) {
        Ok(mut messages) => {
            tracing::info!("get_session_messages: {} 条消息", messages.len());
            for (i, m) in messages.iter().enumerate() {
                tracing::info!("  消息[{}]: role={}, content_prev30={}", i, m.role, &m.content.chars().take(30).collect::<String>());
            }
            messages.truncate(limit);
            Json(serde_json::json!({ "success": true, "data": messages }))
        }
        Err(e) => {
            tracing::error!("get_session_messages 失败: {}", e);
            Json(serde_json::json!({ "success": false, "message": e.to_string() }))
        }
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/sessions", get(list_sessions).post(create_session))
        // 由于 Axum {param} 语法在此版本不可用，改用查询参数
        .route("/session", get(get_session_messages).delete(delete_session))
}
