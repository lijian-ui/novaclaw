//! Soul 模块 - Agent 灵魂文件加载与管理
//! 
//! 支持多 Agent 的 SOUL.md 文件加载，每个 Agent 都有自己独立的灵魂配置

mod loader;
mod manager;
mod models;

pub use loader::SoulLoader;
pub use manager::SoulManager;
pub use models::{SoulInfo, SoulConfig};

/// Soul 文件路径配置
#[derive(Debug, Clone)]
pub struct SoulPaths {
    /// Agent 默认配置目录
    pub agent_default_dir: std::path::PathBuf,
    /// 默认 Agent 名称
    pub default_agent: String,
}

impl SoulPaths {
    /// 获取默认 Agent 的 Soul 目录
    pub fn default_agent_dir(&self) -> std::path::PathBuf {
        self.agent_default_dir.join(&self.default_agent)
    }

    /// 获取指定 Agent 的 Soul 目录
    pub fn agent_dir(&self, agent_name: &str) -> std::path::PathBuf {
        self.agent_default_dir.join(agent_name)
    }

    /// 获取指定 Agent 的 SOUL.md 路径
    pub fn soul_path(&self, agent_name: &str) -> std::path::PathBuf {
        self.agent_dir(agent_name).join("SOUL.md")
    }

    /// 获取指定 Agent 的配置路径
    pub fn config_path(&self, agent_name: &str) -> std::path::PathBuf {
        self.agent_dir(agent_name).join("agent.toml")
    }
}
