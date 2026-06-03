use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;
use std::io::{Read, Write};
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty, Child as PtyChild};

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
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn PtyChild + Send>,
    pub writer: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
    pub cwd: std::path::PathBuf,
}

impl TerminalSession {
    pub async fn spawn(id: &str, cwd: std::path::PathBuf) -> Result<Self, String> {
        let pty_system = native_pty_system();
        
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open pty: {}", e))?;

        let shell = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            "bash"
        };

        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(cwd.clone());

        if cfg!(target_os = "windows") {
            cmd.args(&["-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass"]);
        } else {
            cmd.args(&["--norc"]);
        }

        #[cfg(windows)]
        {
            cmd.env("LANG", "zh_CN.UTF-8");
        }

        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn command in pty: {}", e))?;

        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take writer: {}", e))?;

        Ok(Self {
            id: id.to_string(),
            master: pty_pair.master,
            child,
            writer: Arc::new(std::sync::Mutex::new(writer)),
            cwd,
        })
    }

    pub async fn kill(&mut self) {
        let _ = self.child.kill();
    }

    pub async fn write_stdin(&mut self, data: &str) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|_| "Failed to lock writer")?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Flush error: {}", e))
    }

    pub async fn send_ctrl_c(&self) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|_| "Failed to lock writer")?;
        writer
            .write_all(b"\x03")
            .map_err(|e| format!("Write error: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Flush error: {}", e))
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
            let welcome = "\x1b[32m--- Jeeves Terminal (Real PTY) ---\x1b[0m\r\n";
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
        let session_lock = session.lock().await;
        
        let reader = match session_lock.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[Terminal] Failed to clone pty reader: {}", e);
                return;
            }
        };

        let ws_clone = ws_sender.clone();
        let cancel_clone = cancel_flag.clone();

        tokio::task::spawn_blocking(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            let rt = tokio::runtime::Handle::current();
            loop {
                if cancel_clone.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = decode_windows_output(&buf[..n]);
                        let msg = serde_json::json!({
                            "type": "stdout",
                            "data": data,
                        }).to_string();

                        let ws_sender = ws_clone.clone();
                        rt.block_on(async move {
                            let mut sender = ws_sender.lock().await;
                            let _ = sender.send(Message::Text(msg)).await;
                        });
                    }
                    Err(_) => break,
                }
            }
        });
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
                        if !data.is_empty() {
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
                        let line = format!("{}\r\n", cmd);
                        if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
                            let mut session_lock = session.lock().await;
                            let _ = session_lock.write_stdin(&line).await;
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
                    "resize" => {
                        let cols = parsed["cols"].as_u64().unwrap_or(80) as u16;
                        let rows = parsed["rows"].as_u64().unwrap_or(24) as u16;
                        if let Some(session) = TERMINAL_MANAGER.get_session(&session_id).await {
                            let session_lock = session.lock().await;
                            let _ = session_lock.master.resize(portable_pty::PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
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
                    serde_json::json!({"type":"error","data": format!("Failed to restart shell: {}", e)}).to_string(),
                ))
                .await;
        }
    }
}
