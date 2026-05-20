//! IM 渠道配置管理
//!
//! 管理 `config/im.json` 配置文件的读写。
//! 每个渠道可以同时配置 Webhook 模式（简单发送）和 Stream 模式（双向通信）。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// IM 渠道配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMChannelConfig {
    /// 渠道唯一标识
    pub id: String,
    /// 渠道显示名称
    pub name: String,
    /// 平台类型（如 "dingtalk", "feishu"）
    pub channel_type: String,
    /// 是否启用
    pub enabled: bool,
    /// 渠道具体配置
    pub config: IMChannelDetail,
}

/// 渠道具体配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IMChannelDetail {
    // ─── Webhook 模式 ───
    /// Webhook URL（钉钉自定义机器人、飞书自定义机器人等）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook: Option<String>,
    /// Webhook 签名密钥
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,

    // ─── Stream 模式（钉钉/飞书官方机器人） ───
    /// 应用 Client ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// 应用 Client Secret
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    // ─── 飞书专用 ───
    /// 飞书 App ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// 飞书 App Secret
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_secret: Option<String>,
    /// 飞书 Agent ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// 飞书 Corp ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corp_id: Option<String>,
}

/// IM 配置文件结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMChannelsConfig {
    pub channels: Vec<IMChannelConfig>,
}

impl Default for IMChannelsConfig {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
        }
    }
}

/// 获取 IM 配置文件路径
pub fn im_config_path() -> PathBuf {
    crate::config::get_config_dir().join("im.json")
}

/// 加载 IM 渠道配置
pub fn load() -> IMChannelsConfig {
    let path = im_config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<IMChannelsConfig>(&content) {
                Ok(config) => {
                    tracing::info!("已加载 IM 渠道配置 ({} 个渠道)", config.channels.len());
                    return config;
                }
                Err(e) => {
                    tracing::error!("解析 IM 渠道配置失败: {} (路径: {:?})", e, path);
                }
            },
            Err(e) => {
                tracing::warn!("读取 IM 渠道配置失败: {} (路径: {:?})", e, path);
            }
        }
    } else {
        tracing::info!("IM 渠道配置文件不存在 {:?}，将使用默认配置", path);
    }
    IMChannelsConfig::default()
}

/// 保存 IM 渠道配置
pub fn save(config: &IMChannelsConfig) -> Result<(), String> {
    let path = im_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入文件失败: {}", e))?;
    tracing::info!("IM 渠道配置已保存到 {:?} ({} 个渠道)", path, config.channels.len());
    Ok(())
}

impl IMChannelConfig {
    /// 判断渠道是否使用 Stream 模式（有 client_id + client_secret）
    pub fn use_stream_mode(&self) -> bool {
        self.config.client_id.is_some() && self.config.client_secret.is_some()
    }

    /// 判断渠道是否使用 Webhook 模式（有 webhook url）
    pub fn use_webhook_mode(&self) -> bool {
        self.config.webhook.is_some()
    }

    /// 获取有效平台类型，旧配置（无 channel_type 字段）从配置字段推断
    pub fn effective_type(&self) -> &str {
        if !self.channel_type.is_empty() {
            return &self.channel_type;
        }
        if self.config.client_id.is_some() || self.config.client_secret.is_some() || self.config.webhook.is_some() {
            return "dingtalk";
        }
        if self.config.app_id.is_some() || self.config.app_secret.is_some() {
            return "feishu";
        }
        "dingtalk"
    }
}
