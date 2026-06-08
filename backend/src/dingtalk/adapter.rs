//! DingTalk → IMAdapter 适配器
//!
//! 将 DingTalkClient 包装为统一的 IMAdapter trait 实现。
//! 已有 dingtalk/ 模块零改动，通过此适配器接入 IM 抽象层。

use crate::dingtalk::card::AICardInstance;
use crate::dingtalk::DingTalkClient;
use crate::error::AppError;
use crate::im::adapter::IMAdapter;
use crate::im::types::{
    ConversationType, IncomingMessage, MessageTarget, PlatformCapabilities, PlatformType,
    SendResult,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 钉钉 IM 适配器
///
/// 包装 DingTalkClient，实现 IMAdapter trait。
/// 每个 DingTalkAdapter 实例对应一个钉钉机器人账号。
pub struct DingTalkAdapter {
    client: Arc<DingTalkClient>,
    /// 当前正在流式回复的卡片（可选）
    current_card: std::sync::Mutex<Option<(AICardInstance, mpsc::UnboundedSender<String>)>>,
    /// 账号标识
    pub account_id: String,
}

impl DingTalkAdapter {
    pub fn new(client: Arc<DingTalkClient>) -> Self {
        let account_id = client.account_id.clone();
        Self {
            client,
            current_card: std::sync::Mutex::new(None),
            account_id,
        }
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
        // 优先使用 sessionWebhook 回复（钉钉独有优化，实时性最好）
        if let Some(webhook) = &original.session_webhook {
            self.client.reply_markdown_via_webhook(webhook, "🤖 Jeeves", text).await?;
        } else {
            // 兜底：通过 REST API 回复，按会话类型选择正确目标
            match original.conversation_type {
                ConversationType::Private => {
                    // 私聊必须使用 sender_staff_id（用户真实ID），不能使用 conversation_id
                    let user_id = original.sender_staff_id.as_deref()
                        .or(original.sender_id.as_deref())
                        .unwrap_or("");
                    if user_id.is_empty() {
                        return Err(AppError::External("钉钉私聊回复失败：缺少发送者ID".to_string()));
                    }
                    self.client.send_private_markdown(vec![user_id.to_string()], "🤖 Jeeves", text).await?;
                }
                ConversationType::Group => {
                    self.client.send_group_markdown(&original.conversation_id, "🤖 Jeeves", text).await?;
                }
            }
        }
        Ok(SendResult::ok())
    }

    /// 启动流式回复：创建 AI Card，返回 Sender 用于推送文本块
    async fn start_stream_reply(
        &self,
        original: &IncomingMessage,
    ) -> Result<mpsc::UnboundedSender<String>, AppError> {
        // 创建并投放卡片（私聊用 sender_staff_id，群聊用 conversation_id）
        let card = match original.conversation_type {
            ConversationType::Private => {
                let user_id = original.sender_staff_id.as_deref().or(original.sender_id.as_deref()).unwrap_or("");
                self.client.card_create(Some(user_id), None).await?
            },
            ConversationType::Group => self.client.card_create(None, Some(&original.conversation_id)).await?,
        };

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let client = self.client.clone();
        let card_for_spawn = card.clone();
        let mut inputing_done = false;

        // 后台任务：逐块更新卡片（节流 800ms）
        tokio::spawn(async move {
            let mut accumulated = String::new();
            let mut last_update = std::time::Instant::now();
            let min_interval = std::time::Duration::from_millis(800);

            while let Some(chunk) = rx.recv().await {
                accumulated.push_str(&chunk);

                if !inputing_done && !accumulated.is_empty() {
                    match client.card_set_inputing(&card_for_spawn, &accumulated).await {
                        Ok(()) => tracing::info!("[AI Card] INPUTING 设置成功"),
                        Err(e) => tracing::warn!("[AI Card] INPUTING 设置失败: {}", e),
                    }
                    inputing_done = true;
                    last_update = std::time::Instant::now();
                    // 不 continue：第一个 chunk 也要推送 streaming 内容
                }

                let now = std::time::Instant::now();
                if now.duration_since(last_update) >= min_interval {
                    tracing::debug!("[AI Card] 流式更新: 内容长度={}", accumulated.len());
                    if let Err(e) = client.card_stream_update(&card_for_spawn, &accumulated, false).await {
                        tracing::warn!("[AI Card] 流式更新失败: {}", e);
                    }
                    last_update = now;
                }
            }

            // 流结束：最后一次更新 + FINISHED
            if inputing_done && !accumulated.is_empty() {
                tracing::info!("[AI Card] 最终更新: 内容长度={}", accumulated.len());
                if let Err(e) = client.card_stream_update(&card_for_spawn, &accumulated, true).await {
                    tracing::warn!("[AI Card] 最终流式更新失败: {}", e);
                }
                if let Err(e) = client.card_set_finished(&card_for_spawn, &accumulated).await {
                    tracing::warn!("[AI Card] FINISHED 设置失败: {}", e);
                }
            } else if !accumulated.is_empty() {
                tracing::warn!("[AI Card] INPUTING 从未成功，尝试直接 FINISHED");
                if let Err(e) = client.card_set_finished(&card_for_spawn, &accumulated).await {
                    tracing::warn!("[AI Card] 兜底 FINISHED 失败: {}", e);
                }
            }
            tracing::info!("AI Card 流式回复后台任务结束");
        });

        // 保存卡片引用
        if let Ok(mut guard) = self.current_card.lock() {
            *guard = Some((card, tx.clone()));
        }

        Ok(tx)
    }

    /// 完成流式回复
    async fn finish_stream_reply(
        &self,
        _original: &IncomingMessage,
    ) -> Result<(), AppError> {
        if let Ok(mut guard) = self.current_card.lock() {
            *guard = None;
        }
        Ok(())
    }

    async fn send_image(
        &self,
        target: &MessageTarget,
        url: &str,
        _caption: Option<&str>,
    ) -> Result<SendResult, AppError> {
        match target.conversation_type {
            ConversationType::Private => {
                self.client
                    .send_private_image(vec![target.conversation_id.clone()], url)
                    .await?;
            }
            ConversationType::Group => {
                self.client
                    .send_group_image(&target.conversation_id, url)
                    .await?;
            }
        }
        Ok(SendResult::ok())
    }

    async fn send_file(
        &self,
        target: &MessageTarget,
        url: &str,
        file_name: &str,
    ) -> Result<SendResult, AppError> {
        tracing::info!("[钉钉] 发送文件: target={}, url存在={}, fileName={}",
            target.conversation_id, !url.is_empty(), file_name);

        match target.conversation_type {
            ConversationType::Private => {
                self.client
                    .send_private_file(vec![target.conversation_id.clone()], url, file_name)
                    .await?;
            }
            ConversationType::Group => {
                self.client
                    .send_group_file(&target.conversation_id, url, file_name)
                    .await?;
            }
        }

        tracing::info!("[钉钉] 文件发送成功");
        Ok(SendResult::ok())
    }

    async fn send_video(
        &self,
        target: &MessageTarget,
        url: &str,
        _caption: Option<&str>,
    ) -> Result<SendResult, AppError> {
        tracing::info!("[钉钉] 发送视频: target={}, url存在={}",
            target.conversation_id, !url.is_empty());

        // 默认时长 0（由钉钉服务端自动获取）
        let duration: i64 = 0;

        match target.conversation_type {
            ConversationType::Private => {
                self.client
                    .send_private_video(vec![target.conversation_id.clone()], url, duration)
                    .await?;
            }
            ConversationType::Group => {
                self.client
                    .send_group_video(&target.conversation_id, url, duration)
                    .await?;
            }
        }

        tracing::info!("[钉钉] 视频发送成功");
        Ok(SendResult::ok())
    }

    async fn send_audio(
        &self,
        target: &MessageTarget,
        url: &str,
    ) -> Result<SendResult, AppError> {
        tracing::info!("[钉钉] 发送音频: target={}, url存在={}",
            target.conversation_id, !url.is_empty());

        // 默认时长 0（由钉钉服务端自动获取）
        let duration: i64 = 0;

        match target.conversation_type {
            ConversationType::Private => {
                self.client
                    .send_private_audio(vec![target.conversation_id.clone()], url, duration)
                    .await?;
            }
            ConversationType::Group => {
                self.client
                    .send_group_audio(&target.conversation_id, url, duration)
                    .await?;
            }
        }

        tracing::info!("[钉钉] 音频发送成功");
        Ok(SendResult::ok())
    }

    /// 回复原始消息并 @ 消息发送者
    async fn reply_with_at(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        // 优先使用 sessionWebhook 回复（实时性最好）
        if let Some(webhook) = &original.session_webhook {
            let user_id = original.sender_staff_id.as_deref()
                .or(original.sender_id.as_deref())
                .unwrap_or("")
                .to_string();
            let at_user_ids = if user_id.is_empty() {
                vec![]
            } else {
                vec![user_id]
            };
            self.client
                .reply_with_at(webhook, text, at_user_ids)
                .await?;
        } else {
            // 兜底：通过 REST API 回复
            return self.reply(original, text).await;
        }
        Ok(SendResult::ok())
    }
}
