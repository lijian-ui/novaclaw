use axum::{routing::{get, put}, Json, Router};
use crate::APP_STATE;
use crate::config::AppConfig;

/// 获取当前配置（从文件重新加载，确保页面刷新后数据最新）
async fn get_config() -> Json<serde_json::Value> {
    let fresh_config = AppConfig::reload();

    {
        let mut state = APP_STATE.write().await;
        state.config = fresh_config.clone();
    }

    Json(serde_json::json!({
        "success": true,
        "data": fresh_config,
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
