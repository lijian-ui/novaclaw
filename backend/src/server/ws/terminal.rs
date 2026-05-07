use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::SinkExt;

/// 终端 WebSocket 处理
async fn ws_terminal_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket))
}

async fn handle_terminal_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                // 转发为终端日志
                let _ = socket
                    .send(Message::Text(format!("terminal output: {}", text)))
                    .await;
            }
            Ok(Message::Close(_)) => break,
            _ => {}
        }
    }
}

pub fn routes() -> Router {
    Router::new().route("/terminal", get(ws_terminal_handler))
}
