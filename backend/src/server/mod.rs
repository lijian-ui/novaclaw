pub mod routes;
pub mod ws;

use axum::{Router, routing::get};
use tower_http::{cors::{CorsLayer, Any}, services::ServeDir};
use std::net::SocketAddr;
use std::path::Path;

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

    // 构建路由：静态文件托管优先于 API 路由
    let app = Router::new()
        .nest("/api", routes::build())
        .nest("/ws", ws::build())
        .fallback_service(static_files_handler())
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

/// 前端静态文件托管处理
fn static_files_handler() -> Router {
    // 按顺序尝试多个可能的前端资源目录
    let possible_dirs = [
        "./dist",           // 开发环境：Vite 构建输出
        "../dist",          // 从 backend 目录运行时
        "./frontend/dist",  // 前端源码目录
        "../frontend/dist", // 从 backend 目录运行时
    ];

    for dir in possible_dirs.iter() {
        if Path::new(dir).exists() {
            tracing::info!("Serving frontend from: {}", dir);
            let dir = dir.to_string();
            return Router::new()
                .nest_service("/", ServeDir::new(&dir))
                .fallback(move || async move { index_handler(&dir).await });
        }
    }

    tracing::warn!("No frontend dist directory found. Static files will not be served.");
    Router::new()
}

/// 首页处理：确保返回 index.html 用于 SPA 路由
async fn index_handler(dir: &str) -> impl axum::response::IntoResponse {
    use axum::response::Html;
    
    let index_path = std::path::Path::new(dir).join("index.html");
    if let Ok(content) = std::fs::read_to_string(&index_path) {
        return Html(content);
    }

    Html("<html><body><h1>NovaClaw</h1><p>Frontend not found</p></body></html>".to_string())
}
