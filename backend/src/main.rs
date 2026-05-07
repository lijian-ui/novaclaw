use novaclaw_backend::{initialize, server};

#[tokio::main]
async fn main() {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "novaclaw_backend=info".into())
        )
        .init();

    tracing::info!("Starting NovaClaw backend server...");

    // 初始化应用状态和工具注册
    initialize().await;

    // 启动 Axum HTTP/WebSocket 服务器
    server::start().await;
}
