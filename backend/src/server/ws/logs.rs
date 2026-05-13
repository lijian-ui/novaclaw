use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::time::{interval, Duration};

use crate::logging;

/// 实时日志推送 WebSocket
/// 客户端连接后，会通过 broadcast channel 接收所有 tracing 日志事件
/// 客户端可以发送设置级别的消息来过滤日志
async fn ws_logs_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_logs_socket(socket))
}

async fn handle_logs_socket(socket: WebSocket) {
    let mut rx = logging::get_broadcaster().subscribe();
    let mut ticker = interval(Duration::from_secs(10));
    let (mut sender, mut receiver) = socket.split();

    // 发送欢迎消息
    let welcome = serde_json::json!({
        "type": "connected",
        "data": {"message": "日志连接已建立"}
    });
    let _ = sender
        .send(Message::Text(welcome.to_string()))
        .await;

    loop {
        tokio::select! {
            // 接收来自 tracing broadcast channel 的日志
            msg = rx.recv() => {
                match msg {
                    Ok(entry) => {
                        let payload = serde_json::json!({
                            "type": "log",
                            "data": entry
                        });
                        if sender.send(Message::Text(payload.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("日志 WS 丢失 {} 条日志", n);
                        let payload = serde_json::json!({
                            "type": "lagged",
                            "data": {"count": n}
                        });
                        let _ = sender.send(Message::Text(payload.to_string())).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // 定期心跳
            _ = ticker.tick() => {
                let heartbeat = serde_json::json!({
                    "type": "heartbeat",
                    "data": {"timestamp": chrono::Utc::now().to_rfc3339()}
                });
                if sender.send(Message::Text(heartbeat.to_string())).await.is_err() {
                    break;
                }
            }

            // 接收来自客户端的消息（例如设置日志级别）
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                            if cmd["type"] == "set_level" {
                                if let Some(level) = cmd["data"]["level"].as_str() {
                                    let _ = logging::set_log_level(level);
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => continue,
                }
            }
        }
    }
}

pub fn routes() -> Router {
    Router::new().route("/logs", get(ws_logs_handler))
}
