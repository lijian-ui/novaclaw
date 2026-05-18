//! IM 平台适配器契约
//!
//! 每个 IM 平台实现此 trait，IMGateway 通过 trait object 统一操作。
//! 参考 Hermes Agent 的 BasePlatformAdapter、OpenClaw 的 ChannelPlugin。

use crate::error::AppError;
use crate::im::types::{IncomingMessage, MessageTarget, PlatformCapabilities, PlatformType, SendResult};
use async_trait::async_trait;

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
}
