use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};

/// 检测当前操作系统并返回默认终端 shell 及启动参数
fn detect_shell() -> (&'static str, &'static [&'static str]) {
    if cfg!(target_os = "windows") {
        // cmd.exe 正确处理管道输入，逐个字符读取，遇到 \r 执行命令
        ("cmd.exe", &["/Q"] as &[&'static str])
    } else {
        ("/bin/bash", &["--login"] as &[&'static str])
    }
}

pub fn routes() -> Router {
    Router::new().route("/terminal", get(ws_terminal_handler))
}

async fn ws_terminal_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket))
}

async fn handle_terminal_socket(socket: WebSocket) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    let (shell, shell_args) = detect_shell();

    // 发送欢迎信息
    {
        let mut sender = ws_sender.lock().await;
        let _ = sender
            .send(Message::Text(
                serde_json::json!({"type":"stdout","data": format!(
                    "\x1b[32m--- NovaClaw 终端 (Shell: {}) ---\x1b[0m\r\n", shell
                )})
                .to_string(),
            ))
            .await;
    }

    // 启动持久化 shell 进程（管道模式）
    let mut child = match tokio::process::Command::new(shell)
        .args(shell_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"type":"error","data":format!("无法启动 Shell: {}", e)})
                        .to_string(),
                ))
                .await;
            return;
        }
    };

    let mut stdin = child.stdin.take().expect("stdin pipe");
    let stdout = child.stdout.take().expect("stdout pipe");
    let stderr = child.stderr.take().expect("stderr pipe");

    // ── 读取 stdout ──
    let snd_out = ws_sender.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = stdout;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let msg = serde_json::json!({"type":"stdout","data": text}).to_string();
                    let mut sender = snd_out.lock().await;
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // ── 读取 stderr ──
    let snd_err = ws_sender.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = stderr;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let msg = serde_json::json!({"type":"stderr","data": text}).to_string();
                    let mut sender = snd_err.lock().await;
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // ── 处理 WebSocket 消息 ──
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let parsed = serde_json::from_str(&text).unwrap_or_else(|_| {
                    serde_json::json!({"type":"stdin","data":text})
                });

                let cmd_type = parsed["type"].as_str().unwrap_or("stdin");
                let data = parsed["data"].as_str().unwrap_or("");

                match cmd_type {
                    "stdin" => {
                        let _ = stdin.write_all(data.as_bytes()).await;
                        let _ = stdin.flush().await;
                    }
                    "exec" => {
                        let _ = stdin.write_all(data.as_bytes()).await;
                        let _ = stdin.write_all(b"\r\n").await;
                        let _ = stdin.flush().await;
                    }
                    "kill" => {
                        let _ = child.kill().await;
                        break;
                    }
                    "resize" => {}
                    _ => {}
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    drop(stdin);
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    let _ = child.kill().await;
}
