//! DingTalk → IMAdapter 适配器
//!
//! 将 DingTalkClient 包装为统一的 IMAdapter trait 实现。
//! 已有 dingtalk/ 模块零改动，通过此适配器接入 IM 抽象层。

use crate::dingtalk::DingTalkClient;
use crate::error::AppError;
use crate::im::adapter::IMAdapter;
use crate::im::types::{
    ConversationType, IncomingMessage, MessageTarget, PlatformCapabilities, PlatformType,
    SendResult,
};
use async_trait::async_trait;
use std::sync::Arc;

/// 钉钉 IM 适配器
///
/// 包装 DingTalkClient，实现 IMAdapter trait。
pub struct DingTalkAdapter {
    client: Arc<DingTalkClient>,
}

impl DingTalkAdapter {
    pub fn new(client: Arc<DingTalkClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl IMAdapter for DingTalkAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::DingTalk
    }

    fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities::dingtalk()
    }

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError> {
        match target.conversation_type {
            ConversationType::Private => {
                self.client
                    .send_private_message(vec![target.conversation_id.clone()], text)
                    .await?;
            }
            ConversationType::Group => {
                self.client
                    .send_group_message(&target.conversation_id, text)
                    .await?;
            }
        }
        Ok(SendResult::ok())
    }

    async fn send_markdown(
        &self,
        target: &MessageTarget,
        title: &str,
        text: &str,
    ) -> Result<SendResult, AppError> {
        match target.conversation_type {
            ConversationType::Private => {
                self.client
                    .send_private_markdown(vec![target.conversation_id.clone()], title, text)
                    .await?;
            }
            ConversationType::Group => {
                self.client
                    .send_group_markdown(&target.conversation_id, title, text)
                    .await?;
            }
        }
        Ok(SendResult::ok())
    }

    async fn reply(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        // 优先使用 sessionWebhook 回复（钉钉独有优化）
        if let Some(webhook) = &original.session_webhook {
            self.client.reply_via_webhook(webhook, text).await?;
        } else {
            // 兜底：通过 send_text 回复
            let target = MessageTarget {
                platform: PlatformType::DingTalk,
                conversation_id: original.conversation_id.clone(),
                conversation_type: original.conversation_type.clone(),
            };
            self.send_text(&target, text).await?;
        }
        Ok(SendResult::ok())
    }
}
