use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

pub fn routes() -> Router {
    Router::new()
        .route("/terminal", get(ws_terminal_handler))
        .route("/terminal_sessions", get(list_sessions_handler))
}

pub struct TerminalManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<TerminalSession>>>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn create_session(&self, cwd: std::path::PathBuf) -> Result<String, String> {
        let session_id = Uuid::new_v4().to_string();
        let session = TerminalSession::spawn(&session_id, cwd).await?;
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), Arc::new(Mutex::new(session)));
        Ok(session_id)
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Option<Arc<Mutex<TerminalSession>>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn remove_session(&self, session_id: &str) {
        if let Some(session) = self.sessions.write().await.remove(session_id) {
            let mut session = session.lock().await;
            session.kill().await;
        }
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.sessions.read().await.keys().cloned().collect()
    }
}

static TERMINAL_MANAGER: once_cell::sync::Lazy<TerminalManager> =
    once_cell::sync::Lazy::new(|| TerminalManager::new());

pub struct TerminalSession {
    pub id: String,
    pub child: tokio::process::Child,
    pub stdin: tokio::process::ChildStdin,
    pub cwd: std::path::PathBuf,
}

impl TerminalSession {
    pub async fn spawn(id: &str, cwd: std::path::PathBuf) -> Result<Self, String> {
        let shell = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            "bash"
        };

        let mut cmd = tokio::process::Command::new(shell);
        cmd.current_dir(&cwd);

        if cfg!(target_os = "windows") {
            cmd.args(&["-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass"]);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        } else {
            cmd.args(&["--norc"]);
        }

        #[cfg(windows)]
        {
            cmd.env("LANG", "zh_CN.UTF-8");
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn shell: {}", e))?;

        let stdin = child.stdin.take().ok_or("Failed to get stdin")?;

        Ok(Self {
            id: id.to_string(),
            child,
            stdin,
            cwd,
        })
    }

    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }

    pub async fn write_stdin(&mut self, data: &str) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        self.stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("Flush error: {}", e))
    }

    #[cfg(unix)]
    pub async fn send_ctrl_c(&self) -> Result<(), String> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        if let Some(pid) = self.child.id() {
            kill(Pid::from_raw(pid as i32), Signal::SIGINT)
                .map_err(|e| format!("Failed to send SIGINT: {}", e))?;
            Ok(())
        } else {
            Err("No child process".to_string())
        }
    }

    #[cfg(windows)]
    pub async fn send_ctrl_c(&self) -> Result<(), String> {
        use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_C_EVENT};
        
        if let Some(pid) = self.child.id() {
            unsafe {
                if GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid) == 0 {
                    return Err("Failed to send Ctrl+C event".to_string());
                }
            }
            Ok(())
        } else {
            Err("No child process".to_string())
        }
    }
}

async fn pipe_output(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    ws_sender: Arc<Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
    cancel: Arc<AtomicBool>,
    is_stderr: bool,
) {
    let mut buf = vec![0u8; 4096];
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        match tokio::io::AsyncReadExt::read(&mut reader, &mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let data = decode_windows_output(&buf[..n]);
                let msg_type = if is_stderr { "stderr" } else { "stdout" };
                if let Ok(mut sender) = ws_sender.try_lock() {
                    let _ = sender
                        .send(Message::Text(
                            serde_json::json!({"type": msg_type, "data": data}).to_string(),
                        ))
                        .await;
                }
            }
            Err(_) => break,
        }
    }
}

fn decode_windows_output(data: &[u8]) -> String {
    #[cfg(windows)]
    {
        let (result, _, _) = encoding_rs::GBK.decode(data);
        result.to_string()
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(data).to_string()
    }
}

async fn ws_terminal_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket))
}

async fn list_sessions_handler() -> impl axum::response::IntoResponse {
    let sessions = TERMINAL_MANAGER.list_sessions().await;
    axum::Json(serde_json::json!({ "sessions": sessions }))
}

async fn handle_terminal_socket(socket: WebSocket) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    tracing::debug!("[Terminal] New WebSocket connection");

    let mut cwd = crate::config::get_workspace_dir();
    if !cwd.exists() {
        cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    }

    let session_id = match TERMINAL_MANAGER.create_session(cwd.clone()).await {
        Ok(id) => {
            tracing::debug!("[Terminal] Created session: {}", id);
            let welcome = "\x1b[32m--- Jeeves Terminal ---\x1b[0m\r\n";
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"type":"stdout","data": welcome, "session_id": id}).to_string(),
                ))
                .await;
            id
        }
        Err(e) => {
            tracing::error!("[Terminal] Failed to create session: {}", e);
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"type":"error","data": format!("Failed to start shell: {}", e)}).to_string(),
                ))
                .await;
            return;
        }
    };

    if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
        let mut session_lock = session.lock().await;
        
        let stdout = match session_lock.child.stdout.take() {
            Some(s) => s,
            None => {
                tracing::error!("[Terminal] stdout 已被占用，无法启动终端会话 {}", session_id);
                return;
            }
        };
        let stderr = match session_lock.child.stderr.take() {
            Some(s) => s,
            None => {
                tracing::error!("[Terminal] stderr 已被占用，无法启动终端会话 {}", session_id);
                return;
            }
        };

        let ws_clone1 = ws_sender.clone();
        let ws_clone2 = ws_sender.clone();
        let cancel1 = cancel_flag.clone();
        let cancel2 = cancel_flag.clone();

        tokio::spawn(async move {
            pipe_output(stdout, ws_clone1, cancel1, false).await;
        });

        tokio::spawn(async move {
            pipe_output(stderr, ws_clone2, cancel2, true).await;
        });

        #[cfg(windows)]
        {
            let _ = session_lock.write_stdin("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\r\n").await;
        }

        let _ = session_lock.write_stdin("\r\n").await;
    }

    while let Some(msg) = ws_receiver.next().await {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
        match msg {
            Ok(Message::Text(text)) => {
                let parsed = serde_json::from_str::<serde_json::Value>(&text)
                    .unwrap_or_else(|_| serde_json::json!({"type":"stdin","data": text}));
                let cmd_type = parsed["type"].as_str().unwrap_or("stdin");

                match cmd_type {
                    "stdin" => {
                        let data = parsed["data"].as_str().unwrap_or("");
                        if data == "\x03" {
                            if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
                                let session_lock = session.lock().await;
                                let _ = session_lock.send_ctrl_c().await;
                            }
                        } else {
                            if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
                                let mut session_lock = session.lock().await;
                                if let Err(e) = session_lock.write_stdin(data).await {
                                    tracing::error!("[Terminal] write error: {}", e);
                                    drop(session_lock);
                                    try_restart_shell(&session_id, &ws_sender, &cwd).await;
                                }
                            }
                        }
                    }
                    "kill" => {
                        tracing::debug!("[Terminal] Kill session: {}", session_id);
                        cancel_flag.store(true, Ordering::Relaxed);
                        TERMINAL_MANAGER.remove_session(&session_id).await;
                        break;
                    }
                    "exec" => {
                        let cmd = parsed["command"].as_str()
                            .or_else(|| parsed["data"].as_str())
                            .unwrap_or("");
                        if !cmd.is_empty() {
                            let line = format!("{}\r\n", cmd);
                            if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
                                let mut session_lock = session.lock().await;
                                let _ = session_lock.write_stdin(&line).await;
                            }
                        }
                    }
                    "cd" => {
                        let target = parsed["path"].as_str().unwrap_or("~");
                        let cd_cmd = format!("cd '{}'\r\n", target);
                        if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
                            let mut session_lock = session.lock().await;
                            let _ = session_lock.write_stdin(&cd_cmd).await;
                        }
                    }
                    _ => tracing::warn!("[Terminal] Unknown msg type: {}", cmd_type),
                }
            }
            Ok(Message::Close(_)) => {
                tracing::debug!("[Terminal] WS closed");
                break;
            }
            Err(e) => {
                tracing::warn!("[Terminal] WS error: {}", e);
                break;
            }
            _ => {}
        }
    }

    cancel_flag.store(true, Ordering::Relaxed);
    TERMINAL_MANAGER.remove_session(&session_id).await;
    tracing::debug!("[Terminal] Session {} cleaned up", session_id);
}

async fn try_restart_shell(
    session_id: &str,
    ws_sender: &Arc<Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
    cwd: &std::path::Path,
) {
    tracing::debug!("[Terminal] Restarting shell for session: {}", session_id);
    TERMINAL_MANAGER.remove_session(session_id).await;

    match TERMINAL_MANAGER.create_session(cwd.to_path_buf()).await {
        Ok(new_id) => {
            tracing::debug!("[Terminal] Shell restarted: {}", new_id);
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"type":"session_restarted","session_id": new_id}).to_string(),
                ))
                .await;
            drop(sender);
            if let Some(session) = TERMINAL_MANAGER.get_session(&new_id).await {
                let mut session_lock = session.lock().await;
                let _ = session_lock.write_stdin("\r\n").await;
            }
        }
        Err(e) => {
            tracing::error!("[Terminal] Failed to restart shell: {}", e);
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"type":"error","data": format!("Shell crashed: {}", e)}).to_string(),
                ))
                .await;
        }
    }
}
