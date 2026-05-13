use novaclaw_backend::{initialize, logging, server};

#[tokio::main]
async fn main() {
    // 初始化日志系统（终端 + 文件持久化 + WebSocket 实时推送）
    logging::init();

    tracing::info!("Starting NovaClaw backend server...");

    // 初始化应用状态和工具注册
    initialize().await;

    // 启动 Axum HTTP/WebSocket 服务器
    server::start().await;
}
