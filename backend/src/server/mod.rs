pub mod routes;
pub mod ws;

use axum::Router;
use tower_http::cors::{CorsLayer, Any, AllowOrigin};
use tower_http::services::ServeDir;
use std::net::SocketAddr;
use std::path::Path;

use crate::APP_STATE;

/// 启动 Axum HTTP/WebSocket 服务器（默认从配置文件读取 host/port）
pub async fn start() {
    start_with_opts(None, None).await
}

/// 启动 Axum HTTP/WebSocket 服务器（支持命令行参数覆盖 host/port）
/// 端口被占用时自动尝试下一个可用端口（最多尝试 MAX_PORT_ATTEMPTS 次）
const MAX_PORT_ATTEMPTS: u16 = 100;

pub async fn start_with_opts(
    host_override: Option<String>,
    port_override: Option<u16>,
) {
    let state = APP_STATE.read().await;
    let base_port = port_override.unwrap_or(state.config.port);
    let host = host_override.unwrap_or_else(|| state.config.host.clone());
    drop(state);

    // CORS 配置：限制为本地来源，避免安全风险
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &axum::http::HeaderValue, _request_parts: &axum::http::request::Parts| {
            let origin_str = origin.to_str().unwrap_or("");
            origin_str == "http://localhost:5173"
                || origin_str == "http://127.0.0.1:5173"
                || origin_str == "http://localhost:3000"
                || origin_str == "http://127.0.0.1:3000"
                || origin_str == "tauri://localhost"
                || origin_str.starts_with("http://localhost")
                || origin_str.starts_with("http://127.0.0.1")
        }))
        .allow_methods(Any)
        .allow_headers(Any);

    // 启动 Cron 调度器
    crate::cron::start_scheduler().await;

    // 构建路由：静态文件托管优先于 API 路由
    let app = Router::new()
        .nest("/api", routes::build())
        .nest("/ws", ws::build())
        .fallback_service(static_files_handler())
        .layer(cors)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // 尝试绑定端口，被占用时自动顺延
    let mut actual_port = base_port;
    let listener = loop {
        let addr: SocketAddr = format!("{}:{}", host, actual_port)
            .parse()
            .expect("Invalid address");

        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                if actual_port != base_port {
                    tracing::warn!(
                        "端口 {} 被占用，已自动切换到端口 {}",
                        base_port,
                        actual_port
                    );
                }
                tracing::info!("NovaClaw server starting on http://{}", addr);
                break listener;
            }
            Err(e) => {
                actual_port += 1;
                if actual_port - base_port >= MAX_PORT_ATTEMPTS {
                    panic!(
                        "无法绑定端口: 在 {}~{} 范围内均被占用 ({})",
                        base_port,
                        base_port + MAX_PORT_ATTEMPTS - 1,
                        e
                    );
                }
                tracing::warn!("端口 {} 绑定失败 ({}), 尝试端口 {}", actual_port - 1, e, actual_port);
            }
        }
    };

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
