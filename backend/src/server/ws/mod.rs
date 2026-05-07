pub mod chat;
pub mod terminal;
pub mod logs;

use axum::Router;

/// 构建所有 WebSocket 路由
pub fn build() -> Router {
    Router::new()
        .merge(chat::routes())
        .merge(terminal::routes())
        .merge(logs::routes())
}
