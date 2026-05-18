use axum::{routing::{get, put}, Json, Router};
use crate::APP_STATE;
use crate::config::AppConfig;

/// 获取当前配置（从内存读取，不重新加载磁盘文件）
async fn get_config() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let config = state.config.clone();
    drop(state);

    Json(serde_json::json!({
        "success": true,
        "data": config,
    }))
}

/// 更新配置
async fn update_config(Json(config): Json<AppConfig>) -> Json<serde_json::Value> {
    let mut state = APP_STATE.write().await;
    state.config = config;

    match state.config.save() {
        Ok(_) => {
            tracing::info!("项目配置已保存");
            Json(serde_json::json!({
                "success": true,
                "message": "配置已更新",
            }))
        }
        Err(e) => {
            tracing::error!("保存配置失败: {}", e);
            Json(serde_json::json!({
                "success": false,
                "message": format!("保存配置失败: {}", e),
            }))
        }
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/config", get(get_config))
        .route("/config", put(update_config))
}
