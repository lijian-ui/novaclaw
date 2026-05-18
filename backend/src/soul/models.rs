//! Soul 数据模型定义

use serde::{Deserialize, Serialize};

/// Soul 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulInfo {
    /// Agent 名称
    pub name: String,
    /// Soul 文件路径
    pub path: std::path::PathBuf,
    /// Soul 内容
    pub content: String,
    /// 是否为默认 Agent
    pub is_default: bool,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 版本号
    pub version: String,
}

/// Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulConfig {
    /// Agent 名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 描述
    pub description: String,
    /// Soul 内容路径
    pub soul_file: String,
    /// 使用的模型
    pub model: Option<String>,
    /// 是否启用
    pub enabled: bool,
}

impl Default for SoulConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            version: "1.0.0".to_string(),
            description: "默认 Agent 配置".to_string(),
            soul_file: "SOUL.md".to_string(),
            model: None,
            enabled: true,
        }
    }
}
