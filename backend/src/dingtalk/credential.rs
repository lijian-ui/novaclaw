//! 钉钉 OAuth2 凭据与 Token 管理

use crate::error::AppError;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 钉钉应用凭据
#[derive(Debug, Clone)]
pub struct DingTalkCredential {
    pub client_id: String,
    pub client_secret: String,
}

impl DingTalkCredential {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }
}

/// OAuth2 Token 响应
#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expireIn")]
    expire_in: u64,
}

/// 缓存的 Token 信息
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub access_token: String,
    /// Token 过期时间点（绝对时间）
    expires_at: Instant,
}

impl TokenInfo {
    /// Token 是否已过期（提前 120 秒过期，留有缓冲）
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// Token 管理器
pub struct TokenManager {
    credential: DingTalkCredential,
    http_client: reqwest::Client,
    cache: RwLock<Option<TokenInfo>>,
}

impl TokenManager {
    pub fn new(credential: DingTalkCredential, http_client: reqwest::Client) -> Self {
        Self {
            credential,
            http_client,
            cache: RwLock::new(None),
        }
    }

    /// 获取有效的 Access Token（优先使用缓存）
    pub async fn get_token(&self) -> Result<String, AppError> {
        // 检查缓存
        {
            let cache = self.cache.read().await;
            if let Some(token_info) = cache.as_ref() {
                if !token_info.is_expired() {
                    return Ok(token_info.access_token.clone());
                }
            }
        }

        // 缓存过期或不存在，重新获取
        let token_info = self.fetch_token().await?;
        let token = token_info.access_token.clone();

        let mut cache = self.cache.write().await;
        *cache = Some(token_info);

        Ok(token)
    }

    /// 从钉钉 API 获取新的 Access Token
    async fn fetch_token(&self) -> Result<TokenInfo, AppError> {
        let url = "https://api.dingtalk.com/v1.0/oauth2/accessToken";

        let body = serde_json::json!({
            "appKey": self.credential.client_id,
            "appSecret": self.credential.client_secret,
        });

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉获取 token 请求失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉获取 token 失败 (HTTP {}): {}",
                status, text
            )));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::External(format!("钉钉解析 token 响应失败: {}", e)))?;

        // expire_in 是秒数，提前 120 秒刷新
        let duration = if token_resp.expire_in > 120 {
            Duration::from_secs(token_resp.expire_in - 120)
        } else {
            Duration::from_secs(token_resp.expire_in)
        };

        Ok(TokenInfo {
            access_token: token_resp.access_token,
            expires_at: Instant::now() + duration,
        })
    }

    /// 获取凭据（用于不需要 token 的 API，如网关连接）
    pub fn credential(&self) -> &DingTalkCredential {
        &self.credential
    }
}
