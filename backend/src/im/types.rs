//! 跨平台 IM 消息类型定义
//!
//! 定义了统一的跨平台消息类型，所有渠道适配器需将平台原生消息转换为这些类型。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 平台类型
///
/// 使用枚举 + Custom 变体，兼顾类型安全和可扩展性。
/// 参考 Hermes Agent 的 Platform 枚举 + _missing_() 动态创建。
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformType {
    /// 钉钉
    DingTalk,
    /// 企业微信
    WeChatWork,
    /// 飞书
    Feishu,
    /// Slack
    Slack,
    /// Discord
    Discord,
    /// Telegram
    Telegram,
    /// 自定义平台（通过 env 配置的热插拔平台）
    #[serde(untagged)]
    Custom(String),
}

impl PlatformType {
    pub fn as_str(&self) -> &str {
        match self {
            PlatformType::DingTalk => "dingtalk",
            PlatformType::WeChatWork => "wecom",
            PlatformType::Feishu => "feishu",
            PlatformType::Slack => "slack",
            PlatformType::Discord => "discord",
            PlatformType::Telegram => "telegram",
            PlatformType::Custom(s) => s.as_str(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "dingtalk" => PlatformType::DingTalk,
            "wecom" | "wechat_work" => PlatformType::WeChatWork,
            "feishu" | "lark" => PlatformType::Feishu,
            "slack" => PlatformType::Slack,
            "discord" => PlatformType::Discord,
            "telegram" => PlatformType::Telegram,
            other => PlatformType::Custom(other.to_string()),
        }
    }
}

impl fmt::Display for PlatformType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 会话类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConversationType {
    /// 私聊（单人）
    #[default]
    Private,
    /// 群聊
    Group,
}

impl ConversationType {
    pub fn from_dingtalk(s: &str) -> Self {
        match s {
            "1" => ConversationType::Private,
            "2" => ConversationType::Group,
            _ => ConversationType::Private,
        }
    }
}

/// 跨平台会话来源标识
///
/// 用来统一所有平台的会话查找和标识。
/// 多账号模式下包含 account_id 以区分不同机器人。
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSource {
    pub account_id: String,
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
}

impl SessionSource {
    pub fn new(account_id: String, platform: PlatformType, conversation_id: String) -> Self {
        Self {
            account_id,
            platform,
            conversation_id,
            sender_id: None,
        }
    }

    pub fn with_sender(mut self, sender_id: String) -> Self {
        self.sender_id = Some(sender_id);
        self
    }
}

impl fmt::Display for SessionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.account_id, self.platform, self.conversation_id)
    }
}

/// 标准化入站消息
///
/// 由各个平台的适配器将平台原生消息转换为此格式。
/// 参考 Hermes Agent 的 MessageEvent 设计。
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// 消息 ID（平台原生）
    pub id: String,
    /// 来源账号 ID（多账号模式下唯一标识机器人）
    pub account_id: String,
    /// 来源平台
    pub platform: PlatformType,
    /// 会话 ID
    pub conversation_id: String,
    /// 发送者 ID（平台内部格式，如钉钉 Stream ID）
    pub sender_id: Option<String>,
    /// 发送者员工 ID（钉钉真实用户 ID，用于卡片投放）
    pub sender_staff_id: Option<String>,
    /// 发送者昵称
    pub sender_name: Option<String>,
    /// 消息文本内容
    pub text: String,
    /// 媒体资源 URL 列表
    pub media_urls: Vec<String>,
    /// 原始消息 JSON（调试/转发用）
    pub raw: serde_json::Value,
    /// 会话 Webhook URL（钉钉独有，用于快速回复）
    pub session_webhook: Option<String>,
    /// 会话类型
    pub conversation_type: ConversationType,
    /// 群聊名称
    pub conversation_title: Option<String>,
    /// 消息时间戳（毫秒）
    pub timestamp: i64,
}

/// 消息发送目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTarget {
    /// 目标账号 ID
    pub account_id: String,
    /// 目标平台
    pub platform: PlatformType,
    /// 会话 ID
    pub conversation_id: String,
    /// 会话类型（私聊/群聊）
    #[serde(default)]
    pub conversation_type: ConversationType,
}

impl MessageTarget {
    pub fn new(account_id: String, platform: PlatformType, conversation_id: String) -> Self {
        Self {
            account_id,
            platform,
            conversation_id,
            conversation_type: ConversationType::Private,
        }
    }
}

/// 发送结果
#[derive(Debug, Clone)]
pub struct SendResult {
    pub success: bool,
    pub message_id: Option<String>,
    pub error: Option<String>,
}

impl SendResult {
    pub fn ok() -> Self {
        Self {
            success: true,
            message_id: None,
            error: None,
        }
    }

    pub fn fail(error: String) -> Self {
        Self {
            success: false,
            message_id: None,
            error: Some(error),
        }
    }
}

/// 平台能力声明
///
/// 参考 OpenClaw 的 ChannelCapabilities 设计。
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// 是否支持 Markdown 渲染
    pub supports_markdown: bool,
    /// 是否支持发送图片
    pub supports_images: bool,
    /// 是否支持发送文件
    pub supports_files: bool,
    /// 单条消息最大长度
    pub max_message_length: usize,
}

impl PlatformCapabilities {
    /// 钉钉机器人能力
    pub fn dingtalk() -> Self {
        Self {
            supports_markdown: true,
            supports_images: false, // 钉钉图片需先上传 media_id，暂不支持
            supports_files: false,
            max_message_length: 20000,
        }
    }

    /// 最简能力（纯文本）
    pub fn text_only() -> Self {
        Self {
            supports_markdown: false,
            supports_images: false,
            supports_files: false,
            max_message_length: 4096,
        }
    }
}
