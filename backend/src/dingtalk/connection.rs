//! 钉钉 WebSocket 连接生命周期管理
//!
//! 负责 WebSocket 的建立、读写、心跳保持和自动重连。

use crate::dingtalk::credential::TokenManager;
use crate::dingtalk::frames::{
    AckHeaders, AckMessage, CallbackMessageData, DownStreamMessage, MessageHeaders, PingMessage,
    SystemTopic, CODE_OK,
};
use crate::dingtalk::gateway::GatewayConnector;
use crate::dingtalk::handler::HandlerRegistry;
use crate::error::AppError;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;

/// WebSocket 连接配置
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub auto_reconnect: bool,
    pub reconnect_interval_secs: u64,
    pub keep_alive_interval_secs: u64,
    pub subscribe_robot: bool,
    pub subscribe_card: bool,
    pub subscribe_delegate: bool,
    pub local_ip: Option<String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            reconnect_interval_secs: 10,
            keep_alive_interval_secs: 60,
            subscribe_robot: true,
            subscribe_card: false,
            subscribe_delegate: false,
            local_ip: None,
        }
    }
}

/// WebSocket 连接管理器（外部 handle）
pub struct DingTalkConnection {
    /// broadcast 写入通道 — 外部通过它向 WebSocket 发消息，支持跨重连订阅
    write_tx: broadcast::Sender<String>,
    connected: Arc<AtomicBool>,
    registered: Arc<AtomicBool>,
}

impl DingTalkConnection {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn is_registered(&self) -> bool {
        self.registered.load(Ordering::Relaxed)
    }

    /// 向 WebSocket 发送原始 JSON（通常用于 ACK 或自定义消息）
    pub fn send_raw(&self, json: String) -> Result<(), AppError> {
        self.write_tx.send(json).map_err(|_| {
            AppError::External("钉钉 broadcast 通道已关闭".to_string())
        })?;
        Ok(())
    }

    pub fn connected_flag(&self) -> Arc<AtomicBool> {
        self.connected.clone()
    }
}

/// 启动 WebSocket 连接生命周期（后台任务）
pub async fn start_connection(
    http_client: reqwest::Client,
    token_manager: Arc<TokenManager>,
    handler_registry: Arc<HandlerRegistry>,
    config: ConnectionConfig,
) -> DingTalkConnection {
    let connected = Arc::new(AtomicBool::new(false));
    let registered = Arc::new(AtomicBool::new(false));
    let (write_tx, _) = broadcast::channel::<String>(4096);

    let handle = DingTalkConnection {
        write_tx: write_tx.clone(),
        connected: connected.clone(),
        registered: registered.clone(),
    };

    tokio::spawn(async move {
        loop {
            let write_rx = write_tx.subscribe();

            let write_rx_clone = write_rx.resubscribe();
            let result = run_one_session(
                &http_client,
                &token_manager,
                handler_registry.clone(),
                config.clone(),
                connected.clone(),
                registered.clone(),
                write_rx_clone,
            )
            .await;

            connected.store(false, Ordering::Relaxed);
            registered.store(false, Ordering::Relaxed);
            handler_registry.notify_disconnected();

            match &result {
                Err(e) => {
                    tracing::error!("钉钉连接异常: {}，{}秒后重连", e, config.reconnect_interval_secs);
                    handler_registry.notify_error(&e.to_string());
                }
                Ok(()) => {
                    tracing::info!("钉钉连接正常关闭，{}秒后重连", config.reconnect_interval_secs);
                }
            }

            if !config.auto_reconnect {
                tracing::warn!("自动重连已禁用");
                break;
            }

            handler_registry.notify_reconnecting();
            tokio::time::sleep(Duration::from_secs(config.reconnect_interval_secs)).await;
        }
    });

    handle
}

/// 单次会话
async fn run_one_session(
    http_client: &reqwest::Client,
    token_manager: &TokenManager,
    handler_registry: Arc<HandlerRegistry>,
    config: ConnectionConfig,
    connected: Arc<AtomicBool>,
    registered: Arc<AtomicBool>,
    mut write_rx: broadcast::Receiver<String>,
) -> Result<(), AppError> {
    let local_ip = config
        .local_ip
        .clone()
        .unwrap_or_else(|| get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string()));

    tracing::info!("正在连接钉钉网关 (IP: {})...", local_ip);
    let conn_resp = GatewayConnector::open(
        http_client,
        token_manager.credential().client_id.as_str(),
        token_manager.credential().client_secret.as_str(),
        &local_ip,
        config.subscribe_robot,
        config.subscribe_card,
        config.subscribe_delegate,
    )
    .await?;

    tracing::info!("网关连接成功, endpoint={}", conn_resp.endpoint);

    let ws_url = format!("{}?ticket={}", conn_resp.endpoint, conn_resp.ticket);
    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| AppError::External(format!("WebSocket 连接失败: {}", e)))?;

    tracing::info!("WebSocket 已建立");
    connected.store(true, Ordering::Relaxed);
    handler_registry.notify_connected();

    let (ws_writer, ws_reader) = ws_stream.split();

    // 内部 mpsc 通道
    let (ws_tx, ws_rx) = mpsc::unbounded_channel::<String>();
    let (ack_tx, ack_rx) = mpsc::unbounded_channel::<String>();

    // 写任务：合并 ws_rx + ack_rx → WebSocket
    let write_handle = tokio::spawn(ws_write_loop(ws_writer, ws_rx, ack_rx));

    // 心跳
    let keepalive_interval = config.keep_alive_interval_secs;
    let keepalive_handle = {
        let ws_tx = ws_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(keepalive_interval)).await;
                let ping = serde_json::to_string(&PingMessage {
                    code: CODE_OK,
                    message: "ping".to_string(),
                })
                .unwrap_or_default();
                if ws_tx.send(ping).is_err() {
                    break;
                }
            }
        })
    };

    // 读任务
        let read_handle = ws_read_loop(
            ws_reader,
            ack_tx.clone(),
            &*handler_registry,
            registered.clone(),
        );

    // 转发外部 broadcast → 内部 ws_tx
    let forward_handle = tokio::spawn(async move {
        loop {
            match write_rx.recv().await {
                Ok(msg) => {
                    if ws_tx.send(msg).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("广播丢失 {} 条消息", n);
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    tokio::select! {
        _ = write_handle => tracing::info!("写任务退出"),
        _ = read_handle => tracing::info!("读任务退出"),
        _ = keepalive_handle => tracing::info!("心跳任务退出"),
        _ = forward_handle => tracing::info!("转发任务退出"),
    }

    Ok(())
}

/// 写循环：合并两个 mpsc 通道写入 WebSocket
async fn ws_write_loop(
    mut ws_writer: futures_util::stream::SplitSink<
        WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
        Message,
    >,
    mut ws_rx: mpsc::UnboundedReceiver<String>,
    mut ack_rx: mpsc::UnboundedReceiver<String>,
) {
    use futures_util::future::Either;
    loop {
        let result = tokio::select! {
            msg = ws_rx.recv() => msg.map(Either::Left),
            ack = ack_rx.recv() => ack.map(Either::Right),
        };
        match result {
            Some(Either::Left(msg)) | Some(Either::Right(msg)) => {
                if ws_writer.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            None => break,
        }
    }
}

/// 读循环
async fn ws_read_loop(
    mut ws_reader: futures_util::stream::SplitStream<
        WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    >,
    ack_tx: mpsc::UnboundedSender<String>,
    handler_registry: &HandlerRegistry,
    registered: Arc<AtomicBool>,
) {
    while let Some(result) = ws_reader.next().await {
        match result {
            Ok(Message::Text(text)) => {
                if let Err(e) =
                    dispatch_message(&text, &ack_tx, handler_registry, &registered).await
                {
                    tracing::error!("分发消息失败: {}", e);
                }
            }
            Ok(Message::Close(frame)) => {
                tracing::info!("收到 Close: {:?}", frame);
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Binary(_)) | Ok(Message::Frame(_)) => {}
            Err(e) => {
                tracing::error!("读取错误: {}", e);
                break;
            }
        }
    }
}

// ─── 消息分发 ─────────────────────────────────────

async fn dispatch_message(
    text: &str,
    ack_tx: &mpsc::UnboundedSender<String>,
    handler_registry: &HandlerRegistry,
    registered: &AtomicBool,
) -> Result<(), AppError> {
    let msg: DownStreamMessage = serde_json::from_str(text).map_err(|e| {
        AppError::External(format!("解析消息失败: {} (raw: {})", e, &text[..text.len().min(200)]))
    })?;

    match msg.msg_type.as_str() {
        "SYSTEM" => handle_system(&msg, ack_tx, handler_registry, registered).await,
        "EVENT" => handle_event(&msg, ack_tx).await,
        "CALLBACK" => handle_callback(&msg, ack_tx, handler_registry).await,
        other => {
            tracing::warn!("未知消息类型: {}", other);
            send_ack(ack_tx, &msg.headers, CODE_OK, "OK").ok();
            Ok(())
        }
    }
}

async fn handle_system(
    msg: &DownStreamMessage,
    ack_tx: &mpsc::UnboundedSender<String>,
    handler_registry: &HandlerRegistry,
    registered: &AtomicBool,
) -> Result<(), AppError> {
    match SystemTopic::from_str(&msg.headers.topic) {
        SystemTopic::Connected => {
            tracing::info!("➡ 钉钉: Connected");
            send_ack(ack_tx, &msg.headers, CODE_OK, "OK")?;
        }
        SystemTopic::Registered => {
            tracing::info!("➡ 钉钉: Registered");
            registered.store(true, Ordering::Relaxed);
            handler_registry.notify_registered();
            send_ack(ack_tx, &msg.headers, CODE_OK, "OK")?;
        }
        SystemTopic::Disconnect => {
            tracing::info!("➡ 钉钉: Disconnect");
            send_ack(ack_tx, &msg.headers, CODE_OK, "OK")?;
        }
        SystemTopic::KeepAlive => {
            send_ack(ack_tx, &msg.headers, CODE_OK, "OK")?;
        }
        SystemTopic::Ping => {
            tracing::debug!("➡ 钉钉: Ping");
            let mid = msg.headers.message_id.clone().unwrap_or_default();
            let ack = AckMessage {
                code: CODE_OK,
                headers: AckHeaders {
                    message_id: mid,
                    content_type: "application/json".to_string(),
                },
                message: "OK".to_string(),
                data: Some(msg.data.to_string()),
            };
            let json = serde_json::to_string(&ack).map_err(|e| AppError::Internal(e.to_string()))?;
            ack_tx
                .send(json)
                .map_err(|_| AppError::External("ACK通道关闭".to_string()))?;
        }
        SystemTopic::Unknown(t) => {
            tracing::debug!("未知系统主题: {}", t);
            send_ack(ack_tx, &msg.headers, CODE_OK, "OK")?;
        }
    }
    Ok(())
}

async fn handle_event(
    msg: &DownStreamMessage,
    ack_tx: &mpsc::UnboundedSender<String>,
) -> Result<(), AppError> {
    tracing::debug!("事件: topic={}, eventType={:?}", msg.headers.topic, msg.headers.event_type);
    send_ack(ack_tx, &msg.headers, CODE_OK, "OK")
}

async fn handle_callback(
    msg: &DownStreamMessage,
    ack_tx: &mpsc::UnboundedSender<String>,
    handler_registry: &HandlerRegistry,
) -> Result<(), AppError> {
    let cb: CallbackMessageData = serde_json::from_value(msg.data.clone()).map_err(|e| {
        AppError::External(format!("解析回调数据失败: {} (topic={})", e, msg.headers.topic))
    })?;

    let webhook = cb.session_webhook.clone();
    tracing::debug!("回调: type={}, sender={}", cb.msgtype, cb.sender_nick.as_deref().unwrap_or("?"));
    handler_registry.dispatch_callback(cb, webhook).await;
    send_ack(ack_tx, &msg.headers, CODE_OK, "OK")
}

fn send_ack(
    ack_tx: &mpsc::UnboundedSender<String>,
    headers: &MessageHeaders,
    code: u16,
    msg: &str,
) -> Result<(), AppError> {
    let mid = headers.message_id.clone().unwrap_or_else(|| "unknown".to_string());
    let ack = AckMessage {
        code,
        headers: AckHeaders {
            message_id: mid,
            content_type: "application/json".to_string(),
        },
        message: msg.to_string(),
        data: None,
    };
    let json = serde_json::to_string(&ack).map_err(|e| AppError::Internal(e.to_string()))?;
    ack_tx
        .send(json)
        .map_err(|_| AppError::External("ACK通道关闭".to_string()))
}

fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:53").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}
