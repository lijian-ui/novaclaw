use axum::{
    extract::Path,
    routing::{delete, get},
    Json, Router,
};
use crate::APP_STATE;

/// 列出所有技能
async fn list_skills() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let skills = state.skills_loader.list_skills();
    // 转换为前端期望格式
    let skill_list: Vec<serde_json::Value> = skills
        .into_iter()
        .map(|s| serde_json::json!({
            "id": s.name,
            "name": s.name,
            "description": s.description,
            "version": s.version,
            "level": 0,
            "enabled": s.enabled,
            "content": s.content,
        }))
        .collect();

    Json(serde_json::json!({ "success": true, "data": skill_list }))
}

/// 获取指定技能
async fn get_skill(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.skills_loader.get_skill(&id) {
        Some(skill) => Json(serde_json::json!({
            "success": true,
            "data": {
                "id": skill.name,
                "name": skill.name,
                "description": skill.description,
                "version": skill.version,
                "level": 0,
                "enabled": skill.enabled,
                "content": skill.content,
            }
        })),
        None => Json(serde_json::json!({ "success": false, "message": "技能未找到" })),
    }
}

/// 删除技能
async fn delete_skill(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.skills_loader.delete_skill(&id) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e })),
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/skills", get(list_skills))
        .route("/skills/{id}", get(get_skill))
        .route("/skills/{id}", delete(delete_skill))
}
