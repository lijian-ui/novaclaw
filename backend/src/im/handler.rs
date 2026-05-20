//! IM 入站消息处理器

use crate::dingtalk::frames::CallbackMessageData;
use crate::im::types::{ConversationType, IncomingMessage, PlatformType};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// IMGateway 回调处理器（钉钉消息 → IncomingMessage → IMGateway）
pub struct IMGatewayCallbackHandler {
    incoming_tx: mpsc::UnboundedSender<IncomingMessage>,
}

impl IMGatewayCallbackHandler {
    pub fn new(incoming_tx: mpsc::UnboundedSender<IncomingMessage>) -> Self {
        Self { incoming_tx }
    }
}

#[async_trait]
impl crate::dingtalk::handler::CallbackHandler for IMGatewayCallbackHandler {
    async fn on_callback_message(
        &self,
        msg: CallbackMessageData,
        _session_webhook: Option<String>,
    ) {
        let incoming_msg = IncomingMessage {
            id: msg.msg_id.clone().unwrap_or_default(),
            platform: PlatformType::DingTalk,
            conversation_id: msg
                .conversation_id
                .clone()
                .unwrap_or_else(|| msg.sender_id.clone().unwrap_or_default()),
            sender_id: msg.sender_id.clone(),
            sender_staff_id: msg.sender_staff_id.clone(),
            sender_name: msg.sender_nick.clone(),
            text: msg.text.as_ref().map(|t| t.content.clone()).unwrap_or_default(),
            media_urls: Vec::new(),
            raw: serde_json::to_value(&msg).unwrap_or_default(),
            session_webhook: msg.session_webhook.clone(),
            conversation_type: msg.conversation_type.as_deref().map(ConversationType::from_dingtalk).unwrap_or(ConversationType::Private),
            conversation_title: msg.conversation_title.clone(),
            timestamp: msg.create_at.unwrap_or(0),
        };

        if let Err(e) = self.incoming_tx.send(incoming_msg) {
            tracing::error!("发送入站消息到 IMGateway 失败: {}", e);
        }
    }
}
