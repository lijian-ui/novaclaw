pub mod terminal;
pub mod logs;
pub mod files;

use axum::Router;

/// 构建所有 WebSocket 路由
pub fn build() -> Router {
    Router::new()
        .merge(terminal::routes())
        .merge(files::routes())
        .merge(logs::routes())
}
