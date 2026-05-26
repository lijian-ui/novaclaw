pub mod config;
pub mod sessions;
pub mod models;
pub mod skills;
pub mod cron;
pub mod chat;
pub mod files;
pub mod logs;
pub mod mcp;
pub mod im;
pub mod mentions;
pub mod tools;
pub mod weixin_login;

use axum::{response::IntoResponse, Router};

/// 构建所有 HTTP 路由
pub fn build() -> Router {
    Router::new()
        .merge(config::routes())
        .merge(sessions::routes())
        .merge(models::routes())
        .merge(skills::routes())
        .merge(cron::routes())
        .merge(chat::routes())
        .merge(files::routes())
        .merge(logs::routes())
        .merge(mcp::routes())
        .merge(im::routes())
        .merge(weixin_login::routes())
        .merge(mentions::routes())
        .merge(tools::routes())
        .fallback(|req: axum::extract::Request| async move {
            tracing::warn!("未匹配路由: {} {}", req.method(), req.uri());
            (axum::http::StatusCode::NOT_FOUND, "route not found").into_response()
        })
}
