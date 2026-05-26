use axum::{routing::get, Json, Router};
use crate::APP_STATE;

/// 获取所有可用工具列表（含中文展示名）
async fn list_tools() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let tools = state.tool_registry.list_tools_info().await;
    Json(serde_json::json!({ "success": true, "data": tools }))
}

pub fn routes() -> Router {
    Router::new().route("/tools", get(list_tools))
}
