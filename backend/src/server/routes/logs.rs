use axum::{
    extract::Path,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::logging;

// ─── 请求体结构 ────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetLevelReq {
    level: String,
}

// ─── 端点 ────────────────────────────────────────────────────────

/// GET /api/logs - 获取系统日志（支持 level 过滤）
async fn get_system_logs(
    axum::extract::Query(params): axum::extract::Query<LogQuery>,
) -> Json<serde_json::Value> {
    let entries = logging::read_system_logs(params.level.as_deref());
    match entries {
        Ok(entries) => Json(serde_json::json!({
            "success": true,
            "data": entries
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "message": e
        })),
    }
}

#[derive(Deserialize)]
struct LogQuery {
    level: Option<String>,
}

/// GET /api/logs/tasks - 列出所有有日志的任务 ID
async fn list_task_logs() -> Json<serde_json::Value> {
    match logging::list_task_log_ids() {
        Ok(ids) => Json(serde_json::json!({
            "success": true,
            "data": ids
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "message": e
        })),
    }
}

/// GET /api/logs/tasks/:task_id - 获取指定任务的日志
async fn get_task_log(Path(task_id): Path<String>) -> Json<serde_json::Value> {
    let entries = logging::read_task_log(&task_id);
    match entries {
        Ok(entries) => Json(serde_json::json!({
            "success": true,
            "data": entries
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "message": e
        })),
    }
}

/// DELETE /api/logs/tasks/:task_id - 删除指定任务的日志文件
async fn delete_task_log(Path(task_id): Path<String>) -> Json<serde_json::Value> {
    match logging::delete_task_log(&task_id) {
        Ok(_) => {
            tracing::info!("已删除任务日志: {}", task_id);
            Json(serde_json::json!({
                "success": true,
                "message": format!("任务日志 '{}' 已删除", task_id)
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "message": e
        })),
    }
}

/// POST /api/logs/level - 动态切换日志级别
async fn set_log_level(Json(req): Json<SetLevelReq>) -> Json<serde_json::Value> {
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    let level = req.level.to_lowercase();
    if !valid_levels.contains(&level.as_str()) {
        return Json(serde_json::json!({
            "success": false,
            "message": format!("无效的日志级别: {}，有效值: {:?}", level, valid_levels)
        }));
    }

    match logging::set_log_level(&level) {
        Ok(_) => {
            tracing::info!("日志级别已切换为: {}", level);
            Json(serde_json::json!({
                "success": true,
                "message": format!("日志级别已切换为 {}", level)
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "message": e
        })),
    }
}

/// GET /api/logs/tasks/:task_id/delete - 兼容 GET 方式的删除（供前端直接调用）
async fn delete_task_log_get(Path(task_id): Path<String>) -> Json<serde_json::Value> {
    delete_task_log(Path(task_id)).await
}

pub fn routes() -> Router {
    Router::new()
        .route("/logs", get(get_system_logs))
        .route("/logs/level", post(set_log_level))
        .route("/logs/tasks", get(list_task_logs))
        .route("/logs/tasks/{task_id}", get(get_task_log))
        .route("/logs/tasks/{task_id}", delete(delete_task_log))
        .route("/logs/tasks/{task_id}/delete", get(delete_task_log_get))
}
