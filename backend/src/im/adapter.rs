//! IM 平台适配器契约
//!
//! 每个 IM 平台实现此 trait，IMGateway 通过 trait object 统一操作。
//! 参考 Hermes Agent 的 BasePlatformAdapter、OpenClaw 的 ChannelPlugin。

use crate::error::AppError;
use crate::im::types::{IncomingMessage, MessageTarget, PlatformCapabilities, PlatformType, SendResult};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// 流式回复回调
pub struct StreamCallbacks {
    /// 每次收到新文本块时调用
    pub on_chunk: Box<dyn Fn(&str) + Send>,
    /// 回复完成时调用（传入完整内容）
    pub on_complete: Box<dyn Fn(&str) + Send>,
}

/// IM 平台适配器契约
///
/// 定义了平台无关的消息发送接口。
/// 每个渠道适配器（DingTalk、Slack 等）需实现此 trait。
#[async_trait]
pub trait IMAdapter: Send + Sync {
    /// 返回平台类型标识
    fn platform_type(&self) -> PlatformType;

    /// 适配器是否已连接就绪
    fn is_connected(&self) -> bool;

    /// 返回平台能力声明
    fn capabilities(&self) -> PlatformCapabilities;

    /// 发送文本消息到指定目标
    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError>;

    /// 发送 Markdown 消息
    async fn send_markdown(
        &self,
        target: &MessageTarget,
        title: &str,
        text: &str,
    ) -> Result<SendResult, AppError>;

    /// 回复原始消息（利用平台的回复/Webhook 机制）
    async fn reply(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError>;

    /// 发送图片消息（可选）。url 为图片在线地址。
    async fn send_image(
        &self,
        target: &MessageTarget,
        url: &str,
        caption: Option<&str>,
    ) -> Result<SendResult, AppError> {
        // 默认降级：如果平台不支持图片，返回错误
        Err(AppError::External("该平台不支持发送图片".to_string()))
    }

    /// 发送文件消息（可选）。url 为文件在线地址。
    async fn send_file(
        &self,
        target: &MessageTarget,
        url: &str,
        file_name: &str,
    ) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送文件".to_string()))
    }

    /// 发送视频消息（可选）。url 为视频在线地址。
    async fn send_video(
        &self,
        target: &MessageTarget,
        url: &str,
        caption: Option<&str>,
    ) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送视频".to_string()))
    }

    /// 发送音频消息（可选）。url 为音频在线地址。
    async fn send_audio(
        &self,
        target: &MessageTarget,
        url: &str,
    ) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送音频".to_string()))
    }

    /// 回复原始消息并 @ 消息发送者
    async fn reply_with_at(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        // 默认降级到普通 reply
        self.reply(original, text).await
    }

    /// 流式回复（可选）。返回一个 Sender，调用方可通过它发送文本块
    /// 默认实现降级为非流式 reply
    async fn start_stream_reply(
        &self,
        _original: &IncomingMessage,
    ) -> Result<mpsc::UnboundedSender<String>, AppError> {
        Err(AppError::External("该平台不支持流式回复".to_string()))
    }

    /// 完成流式回复
    async fn finish_stream_reply(
        &self,
        _original: &IncomingMessage,
    ) -> Result<(), AppError> {
        Ok(())
    }
}
