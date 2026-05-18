//! 钉钉 IM 集成模块
//!
//! 本模块实现了钉钉流式（Stream）模式的 WebSocket 长连接客户端，
//! 用于接收和发送钉钉机器人消息。
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use dingtalk::{DingTalkClient, ConnectionConfig};
//!
//! let client = DingTalkClient::new(
//!     "your-client-id".to_string(),
//!     "your-client-secret".to_string(),
//!     ConnectionConfig::default(),
//! ).await;
//!
//! // 注册消息处理器（处理收到的用户消息）
//! client.register_handler(my_handler).await;
//!
//! // 发送消息
//! client.send_private_message(vec!["user-id".into()], "你好").await?;
//! ```

pub mod adapter;
pub mod connection;
pub mod credential;
pub mod frames;
pub mod gateway;
pub mod handler;
pub mod message;

use crate::dingtalk::connection::{start_connection, ConnectionConfig, DingTalkConnection};
use crate::dingtalk::credential::TokenManager;
use crate::dingtalk::handler::{CallbackHandler, HandlerRegistry};
use crate::dingtalk::message::MessageSender;
use crate::error::AppError;
use std::sync::Arc;

/// 钉钉客户端（统一外观）
///
/// 包装了 WebSocket 连接管理、Token 管理和消息发送功能。
pub struct DingTalkClient {
    /// WebSocket 连接管理器
    pub connection: DingTalkConnection,
    /// 消息发送器（REST API）
    message_sender: MessageSender,
    /// 处理器注册表（线程安全）
    handler_registry: Arc<HandlerRegistry>,
    /// 钉钉 Client ID（同时也是 RobotCode）
    client_id: String,
}

impl DingTalkClient {
    /// 创建 DingTalk 客户端并自动启动 WebSocket 连接
    pub async fn new(client_id: String, client_secret: String) -> Self {
        let http_client = reqwest::Client::new();
        let handler_registry = Arc::new(HandlerRegistry::new());

        let credential = crate::dingtalk::credential::DingTalkCredential::new(
            client_id.clone(),
            client_secret,
        );
        let token_manager = Arc::new(TokenManager::new(credential, http_client.clone()));

        let connection = start_connection(
            http_client.clone(),
            token_manager.clone(),
            handler_registry.clone(),
            ConnectionConfig::default(),
        )
        .await;

        let message_sender = MessageSender::new(
            http_client,
            token_manager,
            client_id.clone(),
        );

        Self {
            connection,
            message_sender,
            handler_registry,
            client_id,
        }
    }

    /// 创建 DingTalk 客户端（自定义连接配置）
    pub async fn new_with_config(
        client_id: String,
        client_secret: String,
        config: ConnectionConfig,
    ) -> Self {
        let http_client = reqwest::Client::new();
        let handler_registry = Arc::new(HandlerRegistry::new());

        let credential = crate::dingtalk::credential::DingTalkCredential::new(
            client_id.clone(),
            client_secret,
        );
        let token_manager = Arc::new(TokenManager::new(credential, http_client.clone()));

        let connection = start_connection(
            http_client.clone(),
            token_manager.clone(),
            handler_registry.clone(),
            config,
        )
        .await;

        let message_sender = MessageSender::new(
            http_client,
            token_manager,
            client_id.clone(),
        );

        Self {
            connection,
            message_sender,
            handler_registry,
            client_id,
        }
    }

    /// 注册回调消息处理器
    pub async fn register_handler(&self, handler: impl CallbackHandler + 'static) {
        self.handler_registry
            .register_callback(Box::new(handler))
            .await;
    }

    /// 获取处理器注册表引用（用于底层操作）
    pub fn handler_registry(&self) -> &Arc<HandlerRegistry> {
        &self.handler_registry
    }

    // ─── 便捷消息发送 ───────────────────────────────

    /// 发送私聊文本消息
    pub async fn send_private_message(
        &self,
        user_ids: Vec<String>,
        content: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_private_message(user_ids, content)
            .await
    }

    /// 发送群聊文本消息
    pub async fn send_group_message(
        &self,
        open_conversation_id: &str,
        content: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_group_message(open_conversation_id, content)
            .await
    }

    /// 发送私聊 Markdown 消息
    pub async fn send_private_markdown(
        &self,
        user_ids: Vec<String>,
        title: &str,
        text: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_private_markdown(user_ids, title, text)
            .await
    }

    /// 发送群聊 Markdown 消息
    pub async fn send_group_markdown(
        &self,
        open_conversation_id: &str,
        title: &str,
        text: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_group_markdown(open_conversation_id, title, text)
            .await
    }

    /// 通过会话 Webhook 回复文本
    pub async fn reply_via_webhook(
        &self,
        webhook_url: &str,
        content: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .reply_via_webhook(webhook_url, content)
            .await
    }

    /// 通过会话 Webhook 回复 Markdown
    pub async fn reply_markdown_via_webhook(
        &self,
        webhook_url: &str,
        title: &str,
        text: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .reply_markdown_via_webhook(webhook_url, title, text)
            .await
    }

    /// 下载消息中的文件，返回下载 URL
    pub async fn download_file(&self, download_code: &str) -> Result<String, AppError> {
        self.message_sender.download_file(download_code).await
    }

    // ─── 状态查询 ───────────────────────────────────

    /// WebSocket 是否已连接
    pub fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }

    /// 是否已注册到钉钉网关
    pub fn is_registered(&self) -> bool {
        self.connection.is_registered()
    }
}
