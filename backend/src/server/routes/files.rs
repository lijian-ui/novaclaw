use axum::{
    routing::{get, post},
    Json, Router,
};

/// 获取布局
async fn get_layout() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "data": {
            "id": "default",
            "name": "默认布局",
            "content": "{}",
            "user_id": "default",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }
    }))
}

/// 保存布局
async fn save_layout(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "data": {
            "id": "default",
            "name": body["name"].as_str().unwrap_or("默认布局"),
            "content": body["content"].as_str().unwrap_or("{}"),
            "user_id": "default",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }
    }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/layout", get(get_layout))
        .route("/layout", post(save_layout))
}
