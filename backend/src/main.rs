use jeeves_backend::{initialize, logging, server};

#[tokio::main]
async fn main() {
    // 初始化日志系统
    logging::init();

    // 解析命令行参数：--host <addr> --port <port>
    let args: Vec<String> = std::env::args().collect();
    let mut host_override: Option<String> = None;
    let mut port_override: Option<u16> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                i += 1;
                if i < args.len() {
                    host_override = Some(args[i].clone());
                }
            }
            "--port" => {
                i += 1;
                if i < args.len() {
                    if let Ok(p) = args[i].parse::<u16>() {
                        port_override = Some(p);
                    } else {
                        tracing::warn!("Invalid --port value: {}, ignored", args[i]);
                    }
                }
            }
            other => {
                tracing::warn!("Unknown argument: {}, ignored", other);
            }
        }
        i += 1;
    }

    tracing::info!("Starting Jeeves backend server...");

    // 初始化应用状态和工具注册
    initialize().await;

    // 启动 Axum HTTP/WebSocket 服务器
    server::start_with_opts(host_override, port_override).await;
}
