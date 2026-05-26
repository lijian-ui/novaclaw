//! 微信 IM 适配器
//!
//! 实现 IMAdapter trait，将微信 iLink 协议接入 NovaClaw IM 系统。

use crate::error::AppError;
use crate::im::adapter::IMAdapter;
use crate::im::types::{
    ConversationType, IncomingMessage, MessageTarget, PlatformCapabilities, PlatformType,
    SendResult,
};
use crate::weixin::client::WeixinClient;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 微信 IM 适配器
pub struct WeixinAdapter {
    pub client: Arc<WeixinClient>,
    pub account_id: String,
}

impl WeixinAdapter {
    pub fn new(client: Arc<WeixinClient>, account_id: String) -> Self {
        Self { client, account_id }
    }

    /// 启动入站消息监听（长轮询），将消息投递到 IMGateway
    pub fn start_polling(&self, incoming_tx: mpsc::UnboundedSender<IncomingMessage>) {
        let client = self.client.clone();
        let account_id = self.account_id.clone();
        tokio::spawn(async move {
            tracing::info!("[微信] 长轮询已启动: {}", account_id);
            let mut consecutive_failures = 0;

            loop {
                match client.get_updates().await {
                    Ok(resp) => {
                        consecutive_failures = 0;

                        if resp.errcode == Some(-14) {
                            tracing::warn!("[微信] 会话过期，暂停1小时");
                            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                            continue;
                        }

                        if let Some(msgs) = resp.msgs {
                            for msg in msgs {
                                if let Some(incoming) = convert_to_incoming(&msg, &account_id) {
                                    let _ = incoming_tx.send(incoming);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        tracing::warn!("[微信] getUpdates失败 (第{}次): {}", consecutive_failures, e);
                        if consecutive_failures >= 3 {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            consecutive_failures = 0;
                        }
                    }
                }
            }
        });
    }
}

/// 将微信消息转为统一 IncomingMessage
fn convert_to_incoming(msg: &crate::weixin::client::WeixinMessage, account_id: &str) -> Option<IncomingMessage> {
    // 只处理用户消息
    if msg.message_type != Some(1) {
        return None;
    }

    let text = msg.item_list.as_ref()
        .and_then(|items| items.first())
        .and_then(|item| item.text_item.as_ref())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    if text.is_empty() {
        return None;
    }

    let from = msg.from_user_id.as_deref().unwrap_or("");
    let conv_id = msg.group_id.as_deref().unwrap_or(from);

    Some(IncomingMessage {
        id: format!("wx_{}", msg.message_id.unwrap_or(0)),
        account_id: account_id.to_string(),
        platform: PlatformType::Custom("weixin".to_string()),
        conversation_id: conv_id.to_string(),
        sender_id: Some(from.to_string()),
        sender_staff_id: None,
        sender_name: None,
        text,
        media_urls: vec![],
        raw: serde_json::json!(msg),
        session_webhook: None,
        conversation_type: if msg.group_id.is_some() { ConversationType::Group } else { ConversationType::Private },
        conversation_title: None,
        timestamp: msg.create_time_ms.unwrap_or(0),
    })
}

/// 简易 Markdown 转纯文本
fn strip_markdown(md: &str) -> String {
    let mut result = md.to_string();
    // 移除代码块标记
    result = result.replace("```", "");
    // 移除行内代码
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            result.replace_range(start..=start + end, "");
        } else {
            break;
        }
    }
    // 移除 Markdown 链接标记 [text](url)
    let re = regex::Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
    result = re.replace_all(&result, "$1").to_string();
    // 移除标题标记
    result = result.replace("# ", "");
    result = result.replace("## ", "");
    result = result.replace("### ", "");
    // 移除加粗
    result = result.replace("**", "");
    result = result.replace("__", "");
    result
}

#[async_trait]
impl IMAdapter for WeixinAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Custom("weixin".to_string())
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: false,
            supports_images: false,
            supports_files: false,
            max_message_length: 4000,
        }
    }

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError> {
        self.client.send_text(&target.conversation_id, text, None).await?;
        Ok(SendResult::ok())
    }

    async fn send_markdown(&self, _target: &MessageTarget, _title: &str, text: &str) -> Result<SendResult, AppError> {
        let plain = strip_markdown(text);
        self.send_text(_target, &plain).await
    }

    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError> {
        let sender_id = original.sender_id.as_deref().unwrap_or("");
        let ctx = original.raw.get("context_token").and_then(|v| v.as_str());
        self.client.send_text(sender_id, text, ctx).await?;
        Ok(SendResult::ok())
    }

    async fn start_stream_reply(&self, _original: &IncomingMessage) -> Result<mpsc::UnboundedSender<String>, AppError> {
        Err(AppError::External("微信不支持流式回复".to_string()))
    }

    async fn finish_stream_reply(&self, _original: &IncomingMessage) -> Result<(), AppError> {
        Ok(())
    }
}
