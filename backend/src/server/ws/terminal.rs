use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 检测当前操作系统并返回默认终端 shell
fn detect_shell() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        // Windows: 使用 PowerShell Core (pwsh) 或回退到 powershell.exe
        ("powershell.exe", "-Command")
    } else if cfg!(target_os = "macos") {
        ("/bin/zsh", "-c")
    } else {
        // Linux 默认 bash
        ("/bin/bash", "-c")
    }
}

/// 端点: /ws/terminal
async fn ws_terminal_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket))
}

async fn handle_terminal_socket(socket: WebSocket) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    // 当前子进程的 PID（Arc 共享供 kill 使用）
    let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

    // 发送欢迎消息
    {
        let (shell, _) = detect_shell();
        let mut sender = ws_sender.lock().await;
        let _ = sender
            .send(Message::Text(
                serde_json::json!({"type":"stdout","data":format!("[终端已连接] 使用 {} 作为默认 Shell\n", shell)}).to_string(),
            ))
            .await;
        let _ = sender
            .send(Message::Text(
                serde_json::json!({"type":"stdout","data":"输入命令后按回车执行，Ctrl+C 终止当前进程\n"}).to_string(),
            ))
            .await;
    }

    // 处理接收到的消息
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // 解析 JSON 协议: {"type":"exec","command":"..."} / {"type":"kill"}
                let parsed: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => {
                        // 兼容纯文本命令
                        serde_json::json!({"type":"exec","command":text})
                    }
                };

                // 提取命令类型和命令内容（owned 类型，避免 lifetime 问题）
                let cmd_type = parsed["type"].as_str().unwrap_or("exec").to_string();
                let command = parsed["command"].as_str().unwrap_or("").to_string();

                if cmd_type == "exec" {
                    if command.trim().is_empty() {
                        continue;
                    }

                    // 终止之前的进程（如果有）
                    {
                        let mut pid_guard = child_pid.lock().await;
                        if let Some(pid) = pid_guard.take() {
                            let _ = kill_process_by_pid(pid);
                        }
                    }

                    // 回显输入的命令
                    {
                        let mut sender = ws_sender.lock().await;
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({"type":"stdout","data":format!("$ {}\n", command)}).to_string(),
                            ))
                            .await;
                    }

                    // 获取 shell 信息
                    let (shell, shell_arg) = detect_shell();

                    // 特殊命令: clear / cls
                    if command.trim() == "clear" || command.trim() == "cls" {
                        let mut sender = ws_sender.lock().await;
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({"type":"clear","data":""}).to_string(),
                            ))
                            .await;
                        continue;
                    }

                    // 在子进程中执行命令
                    let sender_clone = Arc::clone(&ws_sender);
                    let pid_clone = Arc::clone(&child_pid);
                    let cmd = command.clone();

                    tokio::spawn(async move {
                            let mut proc = tokio::process::Command::new(shell);
                            proc.arg(shell_arg).arg(&cmd).stdout(std::process::Stdio::piped())
                                .stderr(std::process::Stdio::piped())
                                .stdin(std::process::Stdio::null());

                            match proc.spawn() {
                                Ok(mut child) => {
                                    // 保存 PID 供 kill 使用
                                    if let Some(id) = child.id() {
                                        let mut pid_guard = pid_clone.lock().await;
                                        *pid_guard = Some(id);
                                    }

                                    // 异步读取 stdout
                                    let stdout = child.stdout.take();
                                    let stderr = child.stderr.take();

                                    let snd = Arc::clone(&sender_clone);
                                    let stdout_task = tokio::spawn(async move {
                                        if let Some(stdout) = stdout {
                                            use tokio::io::AsyncBufReadExt;
                                            let reader = tokio::io::BufReader::new(stdout);
                                            let mut lines = reader.lines();
                                            while let Ok(Some(line)) = lines.next_line().await {
                                                let mut sender = snd.lock().await;
                                                let _ = sender
                                                    .send(Message::Text(
                                                        serde_json::json!({"type":"stdout","data":format!("{}\n", line)})
                                                            .to_string(),
                                                    ))
                                                    .await;
                                            }
                                        }
                                    });

                                    let snd = Arc::clone(&sender_clone);
                                    let stderr_task = tokio::spawn(async move {
                                        if let Some(stderr) = stderr {
                                            use tokio::io::AsyncBufReadExt;
                                            let reader = tokio::io::BufReader::new(stderr);
                                            let mut lines = reader.lines();
                                            while let Ok(Some(line)) = lines.next_line().await {
                                                let mut sender = snd.lock().await;
                                                let _ = sender
                                                    .send(Message::Text(
                                                        serde_json::json!({"type":"stderr","data":format!("{}\n", line)}
                                                        )
                                                        .to_string(),
                                                    ))
                                                    .await;
                                            }
                                        }
                                    });

                                    // 等待 stdout/stderr 任务完成
                                    let _ = stdout_task.await;
                                    let _ = stderr_task.await;

                                    // 等待进程退出
                                    let status = child.wait().await;
                                    let exit_code = status
                                        .map(|s| s.code().unwrap_or(-1))
                                        .unwrap_or(-1);

                                    // 清除 PID
                                    {
                                        let mut pid_guard = pid_clone.lock().await;
                                        *pid_guard = None;
                                    }

                                    // 发送退出状态
                                    let mut sender = sender_clone.lock().await;
                                    let _ = sender
                                        .send(Message::Text(
                                            serde_json::json!({"type":"exit","code":exit_code}).to_string(),
                                        ))
                                        .await;

                                    if exit_code != 0 {
                                        let exit_msg = format!("\n[进程退出] 代码: {}\n", exit_code);
                                        let _ = sender
                                            .send(Message::Text(
                                                serde_json::json!({"type":"stdout","data": exit_msg}).to_string(),
                                            ))
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    let mut sender = sender_clone.lock().await;
                                    let _ = sender
                                        .send(Message::Text(
                                            serde_json::json!({"type":"error","data":format!("启动进程失败: {}", e)}
                                            )
                                            .to_string(),
                                        ))
                                        .await;
                                }
                            }
                        });
                    } else if cmd_type == "kill" {
                    // 终止当前运行的进程
                    let mut pid_guard = child_pid.lock().await;
                    if let Some(pid) = pid_guard.take() {
                        let killed = kill_process_by_pid(pid);
                        let mut sender = ws_sender.lock().await;
                        if killed {
                            let _ = sender
                                .send(Message::Text(
                                    serde_json::json!({"type":"stdout","data":"\n[进程已终止]\n"}).to_string(),
                                ))
                                .await;
                        }
                    } else {
                        let mut sender = ws_sender.lock().await;
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({"type":"stdout","data":"[没有正在运行的进程]\n"}).to_string(),
                            ))
                            .await;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // 连接关闭时终止子进程
    let mut pid_guard = child_pid.lock().await;
    if let Some(pid) = pid_guard.take() {
        let _ = kill_process_by_pid(pid);
    }
}

/// 根据 PID 终止进程（跨平台）
fn kill_process_by_pid(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        // Windows: 使用 taskkill /F /PID
        let output = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
        output.is_ok()
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix: 发送 SIGTERM，然后 SIGKILL
        use std::process::Command;
        // 先 SIGTERM
        let _ = Command::new("kill").args([&pid.to_string()]).output();
        // 预留一点时间让进程自行退出
        std::thread::sleep(std::time::Duration::from_millis(100));
        // 再 SIGKILL 确保终止
        let result = Command::new("kill").args(["-9", &pid.to_string()]).output();
        result.is_ok()
    }
}

pub fn routes() -> Router {
    Router::new().route("/terminal", get(ws_terminal_handler))
}
