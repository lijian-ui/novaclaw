use axum::{
    extract::{Path, Query},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use crate::APP_STATE;

#[derive(Deserialize)]
struct CreateSessionReq {
    name: String,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize { 50 }

/// 列出所有会话
async fn list_sessions() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.session_store.list_sessions() {
        Ok(sessions) => Json(serde_json::json!({ "success": true, "data": sessions })),
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

/// 获取指定会话
async fn get_session(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.session_store.get_session(&id) {
        Ok(session) => Json(serde_json::json!({ "success": true, "data": session })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

/// 删除会话
async fn delete_session(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.session_store.delete_session(&id) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

/// 获取会话消息
async fn get_messages(
    Path(session_id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.session_store.get_messages(&session_id) {
        Ok(mut messages) => {
            messages.truncate(query.limit);
            Json(serde_json::json!({ "success": true, "data": messages }))
        }
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/sessions", get(list_sessions))
        .route("/sessions", post(create_session))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}", delete(delete_session))
        .route("/sessions/{session_id}/messages", get(get_messages))
}
