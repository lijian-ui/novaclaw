//! IM 渠道配置 API 路由
//!
//! GET  /api/config/im_channels  → 获取所有 IM 渠道配置
//! POST /api/config/im_channels  → 保存 IM 渠道配置

use axum::{routing::{get, post}, Json, Router};

/// 获取 IM 渠道配置列表
async fn get_im_channels() -> Json<serde_json::Value> {
    let config = crate::im::config::load();
    Json(serde_json::json!({
        "success": true,
        "channels": config.channels,
    }))
}

/// 保存 IM 渠道配置
async fn save_im_channels(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let channels = body.get("channels").and_then(|v| v.as_array());

    if channels.is_none() {
        return Json(serde_json::json!({
            "success": false,
            "message": "缺少 'channels' 字段"
        }));
    }

    let config = crate::im::config::IMChannelsConfig {
        channels: channels
            .unwrap()
            .iter()
            .filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect(),
    };

    match crate::im::config::save(&config) {
        Ok(()) => {
            tracing::info!("IM 渠道配置已保存，共 {} 个渠道，正在热加载...", config.channels.len());

            // 热加载：重建所有 IM 连接
            crate::im::reload::reload_gateway().await;

            Json(serde_json::json!({
                "success": true,
                "message": "IM 渠道配置已保存",
                "channel_count": config.channels.len(),
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "message": format!("保存失败: {}", e),
        })),
    }
}

/// 获取支持 IM 渠道类型列表
async fn get_im_channel_types() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "types": [
            {
                "id": "dingtalk",
                "name": "钉钉",
                "icon": "🔔",
                "color": "text-blue-400",
                "webhook_supported": true,
                "stream_supported": true,
                "fields": ["webhook", "secret", "client_id", "client_secret"]
            },
            {
                "id": "feishu",
                "name": "飞书",
                "icon": "📮",
                "color": "text-green-400",
                "webhook_supported": true,
                "stream_supported": true,
                "fields": ["webhook", "secret", "app_id", "app_secret", "agent_id", "corp_id"]
            },
            {
                "id": "wecom",
                "name": "企业微信",
                "icon": "💼",
                "color": "text-purple-400",
                "webhook_supported": true,
                "stream_supported": false,
                "fields": ["webhook"]
            }
        ]
    }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/config/im_channels", get(get_im_channels))
        .route("/config/im_channels", post(save_im_channels))
        .route("/config/im_channel_types", get(get_im_channel_types))
}
