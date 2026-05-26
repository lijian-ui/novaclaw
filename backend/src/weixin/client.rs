//! 微信 iLink Bot API HTTP 客户端
//!
//! 实现 5 个核心端点：getUpdates（长轮询）、sendMessage、getUploadUrl、getConfig、sendTyping

use crate::error::AppError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

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
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
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
    pub account_id: String,
    pub token: Arc<Mutex<String>>,
    pub updates_buf: Arc<Mutex<String>>,
}

impl WeixinClient {
    pub fn new(base_url: String, account_id: String, token: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            base_url,
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
            }]),
            context_token: context_token.map(|s| s.to_string()),
            group_id: None,
            create_time_ms: None,
        };
        self.send_message(&msg).await
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
