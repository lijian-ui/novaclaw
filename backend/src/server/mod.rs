pub mod routes;
pub mod ws;

use axum::Router;
use tower_http::cors::{CorsLayer, Any};
use std::net::SocketAddr;

use crate::APP_STATE;

/// 启动 Axum HTTP/WebSocket 服务器
pub async fn start() {
    let state = APP_STATE.read().await;
    let port = state.config.port;
    let host = state.config.host.clone();
    drop(state);

    // CORS 配置
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 构建路由
    let app = Router::new()
        .nest("/api", routes::build())
        .nest("/ws", ws::build())
        .layer(cors)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .expect("Invalid address");
    
    tracing::info!("NovaClaw server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
