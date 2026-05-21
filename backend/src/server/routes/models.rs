use axum::{
    extract::Path,
    routing::{get, put},
    Json, Router,
};
use crate::APP_STATE;
use crate::config::ModelsConfig;

/// 列出所有模型
async fn list_models() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let mut models = Vec::new();

    for provider in &state.models_config.providers {
        for model_name in &provider.models {
            models.push(serde_json::json!({
                "id": format!("{}/{}", provider.name, model_name),
                "name": model_name,
                "provider": provider.name,
                "context_window": 128000,
                "max_tokens": 4096,
            }));
        }
    }

    tracing::debug!("列出模型: {} 个", models.len());
    Json(serde_json::json!({ "success": true, "data": models }))
}

/// 获取指定模型
async fn get_model(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let parts: Vec<&str> = id.splitn(2, '/').collect();

    let (provider_name, model_name) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        return Json(serde_json::json!({
            "success": false,
            "message": "无效的模型 ID 格式，应为 provider/model"
        }));
    };

    for provider in &state.models_config.providers {
        if provider.name == provider_name {
            if provider.models.contains(&model_name.to_string()) {
                return Json(serde_json::json!({
                    "success": true,
                    "data": {
                        "id": id,
                        "name": model_name,
                        "provider": provider_name,
                        "context_window": 128000,
                        "max_tokens": 4096,
                    }
                }));
            }
        }
    }

    Json(serde_json::json!({
        "success": false,
        "message": "模型未找到"
    }))
}

/// 获取完整的模型配置（直接从内存读取，不重新加载磁盘文件）
async fn get_models_config() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let config = state.models_config.clone();
    let provider_count = config.providers.len();
    let default_model = config.default_model.clone();
    drop(state);

    tracing::debug!(
        "加载模型配置: {} 个提供商, 默认模型: {:?}",
        provider_count,
        default_model,
    );

    Json(serde_json::json!({
        "success": true,
        "data": config,
    }))
}

/// 保存完整的模型配置
async fn save_models_config(Json(config): Json<ModelsConfig>) -> Json<serde_json::Value> {
    let mut state = APP_STATE.write().await;
    state.models_config = config;
    match state.models_config.save() {
        Ok(_) => {
            tracing::info!(
                "模型配置已保存 ({} 个提供商, 默认模型: {:?})",
                state.models_config.providers.len(),
                state.models_config.default_model
            );
            Json(serde_json::json!({ "success": true }))
        }
        Err(e) => {
            tracing::error!("保存模型配置失败: {}", e);
            Json(serde_json::json!({
                "success": false,
                "message": format!("保存失败: {}", e),
            }))
        }
    }
}

/// 设置默认模型
async fn set_default_model(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let model_name = req["model"].as_str().unwrap_or_default().to_string();
    
    let mut state = APP_STATE.write().await;
    state.models_config.default_model = model_name.clone();
    
    match state.models_config.save() {
        Ok(_) => {
            tracing::info!("默认模型已设置为: {:?}", model_name);
            Json(serde_json::json!({ "success": true }))
        }
        Err(e) => {
            tracing::error!("设置默认模型失败: {}", e);
            Json(serde_json::json!({
                "success": false,
                "message": format!("保存失败: {}", e),
            }))
        }
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/models", get(list_models))
        .route("/models/:id", get(get_model))
        .route("/models-config", get(get_models_config))
        .route("/models-config", put(save_models_config))
        .route("/default-model", put(set_default_model))
}
