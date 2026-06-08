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
pub mod card;
pub mod connection;
pub mod credential;
pub mod frames;
pub mod gateway;
pub mod handler;
pub mod message;

use crate::dingtalk::connection::{start_connection, ConnectionConfig, DingTalkConnection};
use crate::dingtalk::credential::TokenManager;
use crate::dingtalk::frames::MediaUploadResponse;
use crate::dingtalk::handler::{CallbackHandler, HandlerRegistry};
use crate::dingtalk::message::MessageSender;
use crate::error::AppError;
use std::sync::Arc;

/// 钉钉客户端（统一外观）
///
/// 包装了 WebSocket 连接管理、Token 管理和消息发送功能。
/// 支持多账号：每个客户端实例对应一个钉钉机器人，通过 account_id 区分。
pub struct DingTalkClient {
    /// 账号标识
    pub account_id: String,
    /// 账号名称
    pub account_name: Option<String>,
    /// WebSocket 连接管理器
    pub connection: DingTalkConnection,
    /// 消息发送器（REST API）
    message_sender: MessageSender,
    /// 卡片消息发送器
    card_sender: card::CardSender,
    /// 处理器注册表（线程安全）
    handler_registry: Arc<HandlerRegistry>,
    /// 钉钉 Client ID（同时也是 RobotCode）
    client_id: String,
}

impl DingTalkClient {
    /// 创建 DingTalk 客户端并自动启动 WebSocket 连接
    pub async fn new(
        account_id: String,
        account_name: Option<String>,
        client_id: String,
        client_secret: String,
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
            ConnectionConfig::default(),
        )
        .await;

        let message_sender = MessageSender::new(
            http_client.clone(),
            token_manager.clone(),
            client_id.clone(),
        );
        let card_sender = card::CardSender::new(
            http_client,
            token_manager,
            client_id.clone(),
        );

        Self {
            account_id,
            account_name,
            connection,
            message_sender,
            card_sender,
            handler_registry,
            client_id,
        }
    }

    /// 创建 DingTalk 客户端（自定义连接配置）
    pub async fn new_with_config(
        account_id: String,
        account_name: Option<String>,
        client_id: String,
        client_secret: String,
        connection_config: ConnectionConfig,
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
            connection_config,
        )
        .await;

        let message_sender = MessageSender::new(
            http_client.clone(),
            token_manager.clone(),
            client_id.clone(),
        );
        let card_sender = card::CardSender::new(
            http_client,
            token_manager,
            client_id.clone(),
        );

        Self {
            account_id,
            account_name,
            connection,
            message_sender,
            card_sender,
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

    /// 下载消息中的媒体文件并转为 Base64 data URL
    pub async fn download_media_to_base64(
        &self,
        download_code: &str,
        mime_type: &str,
    ) -> Result<String, AppError> {
        self.message_sender
            .download_media_to_base64(download_code, mime_type)
            .await
    }

    /// 发送私聊图片消息
    pub async fn send_private_image(
        &self,
        user_ids: Vec<String>,
        photo_url: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_private_image(user_ids, photo_url)
            .await
    }

    /// 发送群聊图片消息
    pub async fn send_group_image(
        &self,
        open_conversation_id: &str,
        photo_url: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_group_image(open_conversation_id, photo_url)
            .await
    }

    /// 发送私聊文件消息
    pub async fn send_private_file(
        &self,
        user_ids: Vec<String>,
        url: &str,
        file_name: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_private_file(user_ids, url, file_name)
            .await
    }

    /// 发送群聊文件消息
    pub async fn send_group_file(
        &self,
        open_conversation_id: &str,
        url: &str,
        file_name: &str,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_group_file(open_conversation_id, url, file_name)
            .await
    }

    /// 发送私聊视频消息
    pub async fn send_private_video(
        &self,
        user_ids: Vec<String>,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_private_video(user_ids, url, duration)
            .await
    }

    /// 发送群聊视频消息
    pub async fn send_group_video(
        &self,
        open_conversation_id: &str,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_group_video(open_conversation_id, url, duration)
            .await
    }

    /// 发送私聊音频消息
    pub async fn send_private_audio(
        &self,
        user_ids: Vec<String>,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_private_audio(user_ids, url, duration)
            .await
    }

    /// 发送群聊音频消息
    pub async fn send_group_audio(
        &self,
        open_conversation_id: &str,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        self.message_sender
            .send_group_audio(open_conversation_id, url, duration)
            .await
    }

    /// 通过会话 Webhook 回复文本（带 @ 某人）
    pub async fn reply_with_at(
        &self,
        webhook_url: &str,
        content: &str,
        at_user_ids: Vec<String>,
    ) -> Result<(), AppError> {
        self.message_sender
            .reply_with_at(webhook_url, content, at_user_ids)
            .await
    }

    /// 通过会话 Webhook 回复 Markdown（带 @ 某人）
    pub async fn reply_markdown_with_at(
        &self,
        webhook_url: &str,
        title: &str,
        text: &str,
        at_user_ids: Vec<String>,
    ) -> Result<(), AppError> {
        self.message_sender
            .reply_markdown_with_at(webhook_url, title, text, at_user_ids)
            .await
    }

    /// 上传媒体文件到钉钉 OAPI
    pub async fn upload_media(
        &self,
        media_type: &str,
        file_data: Vec<u8>,
        file_name: &str,
    ) -> Result<MediaUploadResponse, AppError> {
        self.message_sender
            .upload_media(media_type, file_data, file_name)
            .await
    }

    // ─── AI Card 流式回复 ──────────────────────────

    /// 创建并投放 AI Card
    pub async fn card_create(&self, target_user_id: Option<&str>, target_open_conversation_id: Option<&str>) -> Result<card::AICardInstance, AppError> {
        self.card_sender.create(target_user_id, target_open_conversation_id).await
    }

    /// 设置卡片为 INPUTING 状态
    pub async fn card_set_inputing(&self, card: &card::AICardInstance, content: &str) -> Result<(), AppError> {
        self.card_sender.set_inputing(card, content).await
    }

    /// 流式更新卡片内容
    pub async fn card_stream_update(&self, card: &card::AICardInstance, content: &str, is_finalize: bool) -> Result<(), AppError> {
        self.card_sender.stream_update(card, content, is_finalize).await
    }

    /// 完成卡片
    pub async fn card_set_finished(&self, card: &card::AICardInstance, content: &str) -> Result<(), AppError> {
        self.card_sender.set_finished(card, content).await
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
