pub mod config;
pub mod sessions;
pub mod models;
pub mod skills;
pub mod cron;
pub mod chat;
pub mod files;

use axum::Router;

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
}
