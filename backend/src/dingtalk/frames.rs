//! 钉钉流式 SDK 消息帧类型定义
//!
//! 参考自 `dingtalk-stream-sdk-rust` 项目的帧协议格式。

use serde::{Deserialize, Serialize};

// ─── 话题常量 ───────────────────────────────────────

/// 机器人消息接收话题
pub const TOPIC_ROBOT: &str = "/v1.0/im/bot/messages/get";
/// 机器人消息委派话题
pub const TOPIC_ROBOT_DELEGATE: &str = "/v1.0/im/bot/messages/delegate";
/// 卡片回调话题
pub const TOPIC_CARD: &str = "/v1.0/card/instances/callback";

// ─── 子协议常量 ─────────────────────────────────────

pub const SUBSCRIPTION_SYSTEM: &str = "SYSTEM";
pub const SUBSCRIPTION_EVENT: &str = "EVENT";
pub const SUBSCRIPTION_CALLBACK: &str = "CALLBACK";

// ─── OK 码 ──────────────────────────────────────────

pub const CODE_OK: u16 = 200;
pub const CODE_BAD_REQUEST: u16 = 400;
pub const CODE_NOT_IMPLEMENTED: u16 = 404;
pub const CODE_SYSTEM_EXCEPTION: u16 = 500;

// ─── 消息类型 ───────────────────────────────────────

/// 下行消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    System,
    Event,
    Callback,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::System => "SYSTEM",
            MessageType::Event => "EVENT",
            MessageType::Callback => "CALLBACK",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "SYSTEM" => Some(Self::System),
            "EVENT" => Some(Self::Event),
            "CALLBACK" => Some(Self::Callback),
            _ => None,
        }
    }
}

// ─── 系统消息主题 ───────────────────────────────────

/// 系统消息主题
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemTopic {
    Connected,
    Registered,
    Disconnect,
    KeepAlive,
    Ping,
    Unknown(String),
}

impl SystemTopic {
    pub fn from_str(s: &str) -> Self {
        match s {
            "CONNECTED" => Self::Connected,
            "REGISTERED" => Self::Registered,
            "disconnect" => Self::Disconnect,
            "KEEPALIVE" => Self::KeepAlive,
            "ping" => Self::Ping,
            other => Self::Unknown(other.to_string()),
        }
    }
}

// ─── 网关请求/响应 ─────────────────────────────────

/// 打开网关连接的请求
#[derive(Debug, Serialize)]
pub struct GatewayRequest {
    pub client_id: String,
    pub client_secret: String,
    pub subscriptions: Vec<Subscription>,
    pub ua: String,
    #[serde(rename = "localIp")]
    pub local_ip: String,
}

/// 订阅项
#[derive(Debug, Serialize)]
pub struct Subscription {
    pub topic: String,
    #[serde(rename = "type")]
    pub sub_type: String,
}

/// 网关连接响应
#[derive(Debug, Deserialize)]
pub struct ConnectionResponse {
    pub endpoint: String,
    pub ticket: String,
}

// ─── 下行消息帧 ────────────────────────────────────

/// 从钉钉服务器收到的下行消息帧（WebSocket Text 帧的 JSON 体）
#[derive(Debug, Deserialize)]
pub struct DownStreamMessage {
    #[serde(rename = "specVersion")]
    pub spec_version: String,
    pub headers: MessageHeaders,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: serde_json::Value,
}

/// 消息头
#[derive(Debug, Deserialize, Clone)]
pub struct MessageHeaders {
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
    pub topic: String,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    pub time: Option<String>,
    #[serde(rename = "appId")]
    pub app_id: Option<String>,
    #[serde(rename = "connectionId")]
    pub connection_id: Option<String>,
    #[serde(rename = "eventBornTime")]
    pub event_born_time: Option<i64>,
    #[serde(rename = "eventCorpId")]
    pub event_corp_id: Option<String>,
    #[serde(rename = "eventId")]
    pub event_id: Option<String>,
    #[serde(rename = "eventType")]
    pub event_type: Option<String>,
    #[serde(rename = "eventUnifiedAppId")]
    pub event_unified_app_id: Option<String>,
}

// ─── 回调消息 ──────────────────────────────────────

/// 回调消息的数据载荷
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct CallbackMessageData {
    #[serde(rename = "msgId")]
    pub msg_id: Option<String>,
    #[serde(rename = "conversationType")]
    pub conversation_type: Option<String>,
    #[serde(rename = "conversationId")]
    pub conversation_id: Option<String>,
    #[serde(rename = "conversationTitle")]
    pub conversation_title: Option<String>,
    #[serde(rename = "senderId")]
    pub sender_id: Option<String>,
    #[serde(rename = "senderNick")]
    pub sender_nick: Option<String>,
    #[serde(rename = "senderCorpId")]
    pub sender_corp_id: Option<String>,
    #[serde(rename = "senderStaffId")]
    pub sender_staff_id: Option<String>,
    #[serde(rename = "sessionWebhook")]
    pub session_webhook: Option<String>,
    #[serde(rename = "sessionWebhookExpiredTime")]
    pub session_webhook_expired_time: Option<i64>,
    #[serde(rename = "chatbotCorpId")]
    pub chatbot_corp_id: Option<String>,
    #[serde(rename = "chatbotUserId")]
    pub chatbot_user_id: Option<String>,
    #[serde(rename = "robotCode")]
    pub robot_code: Option<String>,
    pub is_admin: Option<bool>,
    pub sender_platform: Option<String>,
    pub msgtype: String,
    pub text: Option<TextContent>,
    /// 图片/文件/音频/视频等消息的内容
    pub content: Option<serde_json::Value>,
    #[serde(rename = "atUsers")]
    pub at_users: Option<Vec<AtUser>>,
    #[serde(rename = "isInAtList")]
    pub is_in_at_list: Option<bool>,
    #[serde(rename = "createAt")]
    pub create_at: Option<i64>,
}

/// 文本内容
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct TextContent {
    pub content: String,
}

/// @用户
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AtUser {
    #[serde(rename = "dingtalkId")]
    pub dingtalk_id: Option<String>,
    #[serde(rename = "staffId")]
    pub staff_id: Option<String>,
}

/// 图片内容载荷
#[derive(Debug, Deserialize, Clone)]
pub struct PictureContent {
    #[serde(rename = "downloadCode")]
    pub download_code: Option<String>,
    #[serde(rename = "pictureDownloadCode")]
    pub picture_download_code: Option<String>,
}

/// 文件内容载荷
#[derive(Debug, Deserialize, Clone)]
pub struct FileContent {
    #[serde(rename = "downloadCode")]
    pub download_code: Option<String>,
    #[serde(rename = "fileId")]
    pub file_id: Option<String>,
    #[serde(rename = "fileName")]
    pub file_name: Option<String>,
    #[serde(rename = "spaceId")]
    pub space_id: Option<String>,
}

/// 富文本项
#[derive(Debug, Deserialize, Clone)]
pub struct RichTextItem {
    /// "text" 或 "picture"
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub text: Option<String>,
    #[serde(rename = "downloadCode")]
    pub download_code: Option<String>,
    #[serde(rename = "pictureDownloadCode")]
    pub picture_download_code: Option<String>,
}

/// 富文本内容
#[derive(Debug, Deserialize, Clone)]
pub struct RichTextContent {
    #[serde(rename = "richText")]
    pub rich_text: Vec<RichTextItem>,
}

/// 对话类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationType {
    Private,
    Group,
    Unknown(String),
}

impl ConversationType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "1" => Self::Private,
            "2" => Self::Group,
            other => Self::Unknown(other.to_string()),
        }
    }
}

// ─── 上行消息帧（ACK） ─────────────────────────────

/// ACK 确认消息（回复下行消息，通过 WebSocket 发送）
#[derive(Debug, Serialize)]
pub struct AckMessage {
    pub code: u16,
    pub headers: AckHeaders,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// ACK 消息头
#[derive(Debug, Serialize)]
pub struct AckHeaders {
    #[serde(rename = "messageId")]
    pub message_id: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
}

// ─── 心跳消息 ──────────────────────────────────────

/// 客户端发送的心跳 ping 消息
#[derive(Debug, Serialize)]
pub struct PingMessage {
    pub code: u16,
    pub message: String,
}

// ─── Token 响应 ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "expireIn")]
    pub expire_in: u64,
}

// ─── 消息发送（REST API） ──────────────────────────

/// 发送私聊消息
#[derive(Debug, Serialize)]
pub struct PrivateMessageRequest {
    #[serde(rename = "robotCode")]
    pub robot_code: String,
    #[serde(rename = "userIds")]
    pub user_ids: Vec<String>,
    #[serde(rename = "msgParam")]
    pub msg_param: String,
    #[serde(rename = "msgKey")]
    pub msg_key: String,
}

/// 发送群聊消息
#[derive(Debug, Serialize)]
pub struct GroupMessageRequest {
    #[serde(rename = "robotCode")]
    pub robot_code: String,
    #[serde(rename = "openConversationId")]
    pub open_conversation_id: String,
    #[serde(rename = "msgParam")]
    pub msg_param: String,
    #[serde(rename = "msgKey")]
    pub msg_key: String,
}

/// 消息 key 常量
pub const MSG_KEY_TEXT: &str = "sampleText";
pub const MSG_KEY_MARKDOWN: &str = "sampleMarkdown";
pub const MSG_KEY_IMAGE: &str = "sampleImageMsg";
pub const MSG_KEY_LINK: &str = "sampleLink";

/// 文本消息参数
#[derive(Debug, Serialize)]
pub struct TextMsgParam {
    pub content: String,
}

/// Markdown 消息参数
#[derive(Debug, Serialize)]
pub struct MarkdownMsgParam {
    pub title: String,
    pub text: String,
}

/// 图片消息参数
#[derive(Debug, Serialize)]
pub struct ImageMsgParam {
    #[serde(rename = "photoURL")]
    pub photo_url: String,
}

// ─── Webhook 回复 ──────────────────────────────────

/// 通过 sessionWebhook 回复的消息
#[derive(Debug, Serialize)]
pub struct WebhookReply {
    pub msgtype: String,
    pub text: Option<serde_json::Value>,
    pub markdown: Option<serde_json::Value>,
    pub at: Option<serde_json::Value>,
}

impl WebhookReply {
    pub fn text(content: &str) -> Self {
        Self {
            msgtype: "text".to_string(),
            text: Some(serde_json::json!({"content": content})),
            markdown: None,
            at: None,
        }
    }

    pub fn markdown(title: &str, text: &str) -> Self {
        Self {
            msgtype: "markdown".to_string(),
            text: None,
            markdown: Some(serde_json::json!({"title": title, "text": text})),
            at: None,
        }
    }
}

// ─── 媒体上传 ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MediaUploadResponse {
    pub errcode: i64,
    pub errmsg: String,
    #[serde(rename = "media_id")]
    pub media_id: Option<String>,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<i64>,
}

// ─── 文件下载 ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct FileDownloadRequest {
    #[serde(rename = "robotCode")]
    pub robot_code: String,
    #[serde(rename = "downloadCode")]
    pub download_code: String,
}

#[derive(Debug, Deserialize)]
pub struct FileDownloadResponse {
    #[serde(rename = "downloadUrl")]
    pub download_url: String,
}
