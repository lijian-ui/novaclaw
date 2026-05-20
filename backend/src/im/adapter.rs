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
