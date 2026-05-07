use axum::{
    extract::Path,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CronJob {
    id: String,
    name: String,
    schedule: String,
    enabled: bool,
    payload: String,
    created_at: String,
    updated_at: String,
}

/// 列出所有定时任务
async fn list_cron() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "success": true, "data": [] }))
}

/// 创建定时任务
async fn create_cron(Json(job): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let cron_job = CronJob {
        id: id.clone(),
        name: job["name"].as_str().unwrap_or("未命名").to_string(),
        schedule: job["schedule"].as_str().unwrap_or("0 */1 * * *").to_string(),
        enabled: job["enabled"].as_bool().unwrap_or(true),
        payload: job["payload"].as_str().unwrap_or("").to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    Json(serde_json::json!({ "success": true, "data": cron_job }))
}

/// 获取指定定时任务
async fn get_cron(Path(id): Path<String>) -> Json<serde_json::Value> {
    let _ = id;
    Json(serde_json::json!({
        "success": false,
        "message": format!("定时任务 '{}' 未找到", id)
    }))
}

/// 更新定时任务
async fn update_cron(
    Path(_id): Path<String>,
    Json(_data): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "success": true, "message": "定时任务已更新" }))
}

/// 删除定时任务
async fn delete_cron(Path(_id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "success": true }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/cron-jobs", get(list_cron))
        .route("/cron-jobs", post(create_cron))
        .route("/cron-jobs/{id}", get(get_cron))
        .route("/cron-jobs/{id}", put(update_cron))
        .route("/cron-jobs/{id}", delete(delete_cron))
}
