//! 微信 iLink Bot API HTTP 客户端
//!
//! 实现 5 个核心端点：getUpdates（长轮询）、sendMessage、getUploadUrl、getConfig、sendTyping

use crate::error::AppError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ─── 消息项类型 ─────────────────────────────────────────────────
pub mod msg_item_type {
    pub const TEXT: i32 = 1;
    pub const IMAGE: i32 = 2;
    pub const VOICE: i32 = 3;
    pub const FILE: i32 = 4;
    pub const VIDEO: i32 = 5;
}

/// 媒体上传类型（与 UploadMediaType 对应）
pub mod upload_media_type {
    pub const IMAGE: i32 = 1;
    pub const VIDEO: i32 = 2;
    pub const FILE: i32 = 3;
    pub const VOICE: i32 = 4;
}

/// 消息类型
pub mod message_type {
    pub const USER: i32 = 1;
    pub const BOT: i32 = 2;
}

/// 消息状态
pub mod message_state {
    pub const NEW: i32 = 0;
    pub const GENERATING: i32 = 1;
    pub const FINISH: i32 = 2;
}

// ─── 请求/响应类型 ───────────────────────────────────────────

/// 公共请求元数据（附加到每个 CGI 请求）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_agent: Option<String>,
}

impl BaseInfo {
    pub fn new() -> Self {
        Self {
            channel_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            bot_agent: Some("Jeeves".to_string()),
        }
    }
}

impl Default for BaseInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(alias = "message_id")]
    pub message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(alias = "create_time_ms")]
    pub create_time_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
}

/// CDN 媒体引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CDNMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,
}

/// 图片消息项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CDNMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<i64>,
}

/// 文件消息项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CDNMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

/// 视频消息项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CDNMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_size: Option<i64>,
}

/// 语音消息项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CDNMedia>,
    /// 服务端语音转文字结果（若有，则无需下载语音）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// 扩展 MessageItem 以支持媒体类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
}

/// 上传 URL 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUploadUrlReq {
    pub filekey: String,
    #[serde(rename = "media_type")]
    pub media_type: i32,
    #[serde(rename = "to_user_id")]
    pub to_user_id: String,
    pub rawsize: i64,
    pub rawfilemd5: String,
    pub filesize: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_need_thumb: Option<bool>,
    pub aeskey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

/// 上传 URL 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUploadUrlResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_upload_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_full_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesResp {
    pub ret: Option<i32>,
    pub errcode: Option<i32>,
    pub errmsg: Option<String>,
    pub msgs: Option<Vec<WeixinMessage>>,
    pub get_updates_buf: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageReq {
    pub msg: WeixinMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigResp {
    pub ret: Option<i32>,
    pub errmsg: Option<String>,
    pub typing_ticket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTypingReq {
    pub ilink_user_id: String,
    pub typing_ticket: String,
    pub status: i32,
}

/// iLink App ID（从 package.json ilink_appid 获取，无则填空字符串）
const ILINK_APP_ID: &str = "";

/// iLink App 客户端版本号（编码为 0x00MMNNPP）
fn build_app_client_version() -> u32 {
    let parts: Vec<u32> = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|p| p.parse::<u32>().unwrap_or(0))
        .collect();
    let major = parts.first().copied().unwrap_or(0) & 0xff;
    let minor = parts.get(1).copied().unwrap_or(0) & 0xff;
    let patch = parts.get(2).copied().unwrap_or(0) & 0xff;
    (major << 16) | (minor << 8) | patch
}

// ─── iLink 客户端 ────────────────────────────────────────────

/// 微信 iLink 客户端
pub struct WeixinClient {
    http: Client,
    base_url: String,
    pub cdn_base_url: String,
    pub account_id: String,
    pub token: Arc<Mutex<String>>,
    pub updates_buf: Arc<Mutex<String>>,
}

impl WeixinClient {
    pub fn new(base_url: String, cdn_base_url: String, account_id: String, token: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            base_url,
            cdn_base_url,
            account_id,
            token: Arc::new(Mutex::new(token)),
            updates_buf: Arc::new(Mutex::new(String::new())),
        }
    }

    fn uin() -> String {
        use base64::Engine;
        let val: u32 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        base64::engine::general_purpose::STANDARD.encode(&val.to_le_bytes())
    }

    async fn headers(&self) -> Result<reqwest::header::HeaderMap, AppError> {
        let token = self.token.lock().await;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        headers.insert("Authorization", format!("Bearer {}", token).parse().unwrap());
        headers.insert("X-WECHAT-UIN", Self::uin().parse().unwrap());
        headers.insert("iLink-App-Id", ILINK_APP_ID.parse().unwrap());
        headers.insert("iLink-App-ClientVersion", build_app_client_version().to_string().parse().unwrap());
        Ok(headers)
    }

    async fn post<T: Serialize + ?Sized, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
        timeout_secs: u64,
    ) -> Result<R, AppError> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path);
        let resp = self
            .http
            .post(&url)
            .headers(self.headers().await?)
            .json(body)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| AppError::External(format!("微信iLink请求失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "微信iLink HTTP {}: {}", status.as_u16(), text
            )));
        }

        resp.json::<R>()
            .await
            .map_err(|e| AppError::External(format!("解析微信iLink响应失败: {}", e)))
    }

    /// 长轮询拉取新消息（阻塞直到有新消息或超时）
    pub async fn get_updates(&self) -> Result<GetUpdatesResp, AppError> {
        let buf = self.updates_buf.lock().await.clone();
        let req = GetUpdatesReq {
            get_updates_buf: if buf.is_empty() { None } else { Some(buf) },
            base_info: Some(BaseInfo::new()),
        };
        let resp: GetUpdatesResp = self.post("ilink/bot/getupdates", &req, 40).await?;

        // 保存游标
        if let Some(ref new_buf) = resp.get_updates_buf {
            let mut buf = self.updates_buf.lock().await;
            *buf = new_buf.clone();
        }

        Ok(resp)
    }

    /// 发送消息
    pub async fn send_message(&self, msg: &WeixinMessage) -> Result<(), AppError> {
        let _: serde_json::Value = self.post("ilink/bot/sendmessage", &SendMessageReq { msg: msg.clone(), base_info: Some(BaseInfo::new()) }, 15).await?;
        Ok(())
    }

    /// 发送文本消息（自动生成 client_id）
    pub async fn send_text(&self, to_user_id: &str, text: &str, context_token: Option<&str>) -> Result<(), AppError> {
        let client_id = format!("jeeves-weixin:{}", uuid::Uuid::new_v4().to_string());
        let msg = WeixinMessage {
            seq: None,
            message_id: None,
            from_user_id: Some(String::new()),
            to_user_id: Some(to_user_id.to_string()),
            client_id: Some(client_id),
            message_type: Some(2), // BOT
            message_state: Some(2), // FINISH
            item_list: Some(vec![MessageItem {
                item_type: 1, // TEXT
                text_item: Some(TextItem { text: text.to_string() }),
                image_item: None,
                file_item: None,
                video_item: None,
                voice_item: None,
            }]),
            context_token: context_token.map(|s| s.to_string()),
            group_id: None,
            create_time_ms: None,
        };
        self.send_message(&msg).await
    }

    /// 获取文件上传 URL
    ///
    /// 先获取 CDN 上传 URL，用于后续的文件加密上传。
    pub async fn get_upload_url(&self, req: &GetUploadUrlReq) -> Result<GetUploadUrlResp, AppError> {
        let body = serde_json::json!({
            "filekey": req.filekey,
            "media_type": req.media_type,
            "to_user_id": req.to_user_id,
            "rawsize": req.rawsize,
            "rawfilemd5": req.rawfilemd5,
            "filesize": req.filesize,
            "no_need_thumb": req.no_need_thumb.unwrap_or(true),
            "aeskey": req.aeskey,
            "base_info": { "bot_agent": "Jeeves" },
        });
        self.post("ilink/bot/getuploadurl", &body, 15).await
    }

    /// 发送媒体项（图片/视频/文件），可选附带文本标题
    async fn send_media_items(
        &self,
        to_user_id: &str,
        text: &str,
        media_item: MessageItem,
        context_token: Option<&str>,
    ) -> Result<(), AppError> {
        let mut items = Vec::new();
        if !text.is_empty() {
            items.push(MessageItem {
                item_type: msg_item_type::TEXT,
                text_item: Some(TextItem { text: text.to_string() }),
                image_item: None,
                file_item: None,
                video_item: None,
                voice_item: None,
            });
        }
        items.push(media_item);

        let client_id = format!("jeeves-weixin:{}", uuid::Uuid::new_v4().to_string());
        let msg = WeixinMessage {
            seq: None,
            message_id: None,
            from_user_id: Some(String::new()),
            to_user_id: Some(to_user_id.to_string()),
            client_id: Some(client_id),
            message_type: Some(message_type::BOT),
            message_state: Some(message_state::FINISH),
            item_list: Some(items),
            context_token: context_token.map(|s| s.to_string()),
            group_id: None,
            create_time_ms: None,
        };
        self.send_message(&msg).await
    }

    /// 发送图片消息
    ///
    /// `download_param` 来自 CDN 上传后的 `x-encrypted-param` 响应头。
    /// `aes_key_base64` 是 AES 密钥的 base64 编码。
    /// `file_size_ciphertext` 是密文大小。
    pub async fn send_image_message(
        &self,
        to_user_id: &str,
        text: &str,
        download_param: &str,
        aes_key_base64: &str,
        file_size_ciphertext: i64,
        context_token: Option<&str>,
    ) -> Result<(), AppError> {
        let image_item = MessageItem {
            item_type: msg_item_type::IMAGE,
            text_item: None,
            image_item: Some(ImageItem {
                media: Some(CDNMedia {
                    encrypt_query_param: Some(download_param.to_string()),
                    aes_key: Some(aes_key_base64.to_string()),
                    encrypt_type: Some(1),
                    full_url: None,
                }),
                mid_size: Some(file_size_ciphertext),
            }),
            file_item: None,
            video_item: None,
            voice_item: None,
        };
        self.send_media_items(to_user_id, text, image_item, context_token).await
    }

    /// 发送视频消息
    pub async fn send_video_message(
        &self,
        to_user_id: &str,
        text: &str,
        download_param: &str,
        aes_key_base64: &str,
        file_size_ciphertext: i64,
        context_token: Option<&str>,
    ) -> Result<(), AppError> {
        let video_item = MessageItem {
            item_type: msg_item_type::VIDEO,
            text_item: None,
            image_item: None,
            file_item: None,
            video_item: Some(VideoItem {
                media: Some(CDNMedia {
                    encrypt_query_param: Some(download_param.to_string()),
                    aes_key: Some(aes_key_base64.to_string()),
                    encrypt_type: Some(1),
                    full_url: None,
                }),
                video_size: Some(file_size_ciphertext),
            }),
            voice_item: None,
        };
        self.send_media_items(to_user_id, text, video_item, context_token).await
    }

    /// 发送文件消息
    pub async fn send_file_message(
        &self,
        to_user_id: &str,
        text: &str,
        file_name: &str,
        download_param: &str,
        aes_key_base64: &str,
        file_size: i64,
        context_token: Option<&str>,
    ) -> Result<(), AppError> {
        let file_item = MessageItem {
            item_type: msg_item_type::FILE,
            text_item: None,
            image_item: None,
            file_item: Some(FileItem {
                media: Some(CDNMedia {
                    encrypt_query_param: Some(download_param.to_string()),
                    aes_key: Some(aes_key_base64.to_string()),
                    encrypt_type: Some(1),
                    full_url: None,
                }),
                file_name: Some(file_name.to_string()),
                len: Some(file_size.to_string()),
            }),
            video_item: None,
            voice_item: None,
        };
        self.send_media_items(to_user_id, text, file_item, context_token).await
    }

    /// 发送正在输入状态
    pub async fn send_typing(&self, user_id: &str, ticket: &str) -> Result<(), AppError> {
        let req = SendTypingReq {
            ilink_user_id: user_id.to_string(),
            typing_ticket: ticket.to_string(),
            status: 1,
        };
        let _: serde_json::Value = self.post("ilink/bot/sendtyping", &req, 10).await?;
        Ok(())
    }

    /// 获取配置（含 typing_ticket）
    pub async fn get_config(&self, user_id: &str) -> Result<GetConfigResp, AppError> {
        let body = serde_json::json!({
            "ilink_user_id": user_id,
            "base_info": { "bot_agent": "Jeeves" },
        });
        self.post("ilink/bot/getconfig", &body, 10).await
    }
}
