use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::time::{interval, Duration};

/// 实时日志推送 WebSocket
async fn ws_logs_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_logs_socket(socket))
}

async fn handle_logs_socket(mut socket: WebSocket) {
    let mut ticker = interval(Duration::from_secs(2));

    let _ = socket
        .send(Message::Text(
            serde_json::json!({
                "type": "info",
                "data": {"message": "日志连接已建立"}
            })
            .to_string(),
        ))
        .await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // 发送心跳
                if socket
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "heartbeat",
                            "data": {"timestamp": chrono::Utc::now().to_rfc3339()}
                        }).to_string(),
                    ))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

pub fn routes() -> Router {
    Router::new().route("/logs", get(ws_logs_handler))
}
