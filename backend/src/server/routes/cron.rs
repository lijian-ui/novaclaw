use axum::{
    extract::Path,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::cron;

#[derive(Deserialize)]
struct CreateJobReq {
    name: Option<String>,
    schedule: Option<String>,
    payload: Option<String>,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct UpdateJobReq {
    name: Option<String>,
    schedule: Option<String>,
    payload: Option<String>,
    enabled: Option<bool>,
}

async fn list_cron() -> Json<serde_json::Value> {
    let sa = cron::get_store();
    let store = sa.lock().await;
    Json(serde_json::json!({ "success": true, "data": store.list() }))
}

async fn create_cron(Json(req): Json<CreateJobReq>) -> Json<serde_json::Value> {
    let schedule = req.schedule.unwrap_or_else(|| "0 * * * *".to_string());
    let name = req.name.unwrap_or_else(|| "未命名".to_string());
    let payload = req.payload.unwrap_or_default();
    let session_id = req.session_id;
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let next_run = cron::compute_initial_next_run(&schedule);
    let job = cron::CronJob {
        id: id.clone(), name, schedule, enabled: true, payload,
        session_id,
        created_at: now.clone(), updated_at: now,
        last_run_at: None, next_run_at: Some(next_run),
        status: "idle".to_string(), run_count: 0, last_error: None,
    };
    {
        let sa = cron::get_store();
        let mut store = sa.lock().await;
        store.add(job);
    }
    let sa = cron::get_store();
    let store = sa.lock().await;
    let job = store.get(&id).cloned();
    Json(serde_json::json!({ "success": true, "data": job }))
}

async fn get_cron(Path(id): Path<String>) -> Json<serde_json::Value> {
    let sa = cron::get_store();
    let store = sa.lock().await;
    match store.get(&id) {
        Some(job) => Json(serde_json::json!({ "success": true, "data": job })),
        None => Json(serde_json::json!({ "success": false, "message": format!("定时任务 '{}' 未找到", id) })),
    }
}

async fn update_cron(Path(id): Path<String>, Json(req): Json<UpdateJobReq>) -> Json<serde_json::Value> {
    let sa = cron::get_store();
    let mut store = sa.lock().await;
    let updated = store.update(&id, |job| {
        if let Some(name) = req.name { job.name = name; }
        if let Some(schedule) = req.schedule {
            job.schedule = schedule.clone();
            job.next_run_at = Some(cron::compute_initial_next_run(&schedule));
        }
        if let Some(payload) = req.payload { job.payload = payload; }
        if let Some(enabled) = req.enabled { job.enabled = enabled; }
    });
    if updated {
        let job = store.get(&id).cloned();
        Json(serde_json::json!({ "success": true, "data": job }))
    } else {
        Json(serde_json::json!({ "success": false, "message": format!("定时任务 '{}' 未找到", id) }))
    }
}

async fn delete_cron(Path(id): Path<String>) -> Json<serde_json::Value> {
    let sa = cron::get_store();
    let mut store = sa.lock().await;
    let removed = store.remove(&id);
    let _ = crate::logging::delete_task_log(&id);
    if removed {
        tracing::info!("定时任务已删除: {}, 关联日志已清理", id);
        Json(serde_json::json!({ "success": true, "message": "定时任务已删除" }))
    } else {
        Json(serde_json::json!({ "success": false, "message": format!("定时任务 '{}' 未找到", id) }))
    }
}

async fn toggle_cron(Path(id): Path<String>) -> Json<serde_json::Value> {
    let sa = cron::get_store();
    let mut store = sa.lock().await;
    let toggled = store.update(&id, |job| { job.enabled = !job.enabled; });
    if toggled {
        let job = store.get(&id).cloned();
        Json(serde_json::json!({ "success": true, "data": job }))
    } else {
        Json(serde_json::json!({ "success": false, "message": format!("定时任务 '{}' 未找到", id) }))
    }
}

async fn run_cron_now(Path(id): Path<String>) -> Json<serde_json::Value> {
    let sa = cron::get_store();
    let mut store = sa.lock().await;
    if store.get(&id).is_none() {
        return Json(serde_json::json!({ "success": false, "message": format!("定时任务 '{}' 未找到", id) }));
    }
    store.update(&id, |job| {
        job.last_run_at = Some(chrono::Utc::now().to_rfc3339());
        job.run_count += 1;
    });
    Json(serde_json::json!({ "success": true, "message": "任务已触发" }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/cron-jobs", get(list_cron))
        .route("/cron-jobs", post(create_cron))
        .route("/cron-jobs/:id", get(get_cron))
        .route("/cron-jobs/:id", put(update_cron))
        .route("/cron-jobs/:id", delete(delete_cron))
        .route("/cron-jobs/:id/toggle", post(toggle_cron))
        .route("/cron-jobs/:id/run", post(run_cron_now))
}
