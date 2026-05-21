//! Soul 模块 - Agent 灵魂文件加载与管理
//! 
//! 支持多 Agent 的 SOUL.md 文件加载，每个 Agent 都有自己独立的灵魂配置

mod loader;
mod manager;
mod models;

pub use loader::{SoulLoader, SoulError};
pub use manager::SoulManager;
pub use models::{SoulInfo, SoulConfig};
use serde::{Deserialize, Serialize};

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

    /// 获取指定 Agent 的 JSON 配置路径
    pub fn agent_json_path(&self, agent_name: &str) -> std::path::PathBuf {
        self.agent_dir(agent_name).join("agent.json")
    }
}

/// Agent 配置文件（agent.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    #[serde(default = "default_agent_iterations")]
    pub max_iterations: u32,
}

fn default_agent_iterations() -> u32 { 0 }

impl AgentConfig {
    /// 读取指定 Agent 的配置文件
    pub fn load(paths: &SoulPaths, agent_id: &str) -> Result<Self, String> {
        let path = paths.agent_json_path(agent_id);
        if !path.exists() {
            return Err(format!("Agent '{}' 配置文件未找到", agent_id));
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("读取 Agent 配置文件失败: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("解析 Agent 配置文件失败: {}", e))
    }

    /// 保存 Agent 配置文件
    pub fn save(&self, paths: &SoulPaths) -> Result<(), String> {
        let dir = paths.agent_dir(&self.id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建 Agent 目录失败: {}", e))?;
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化 Agent 配置失败: {}", e))?;
        std::fs::write(paths.agent_json_path(&self.id), content)
            .map_err(|e| format!("写入 Agent 配置文件失败: {}", e))
    }

    /// 删除 Agent 目录及所有内容
    pub fn remove(paths: &SoulPaths, agent_id: &str) -> Result<(), String> {
        let dir = paths.agent_dir(agent_id);
        if dir.exists() {
            if agent_id == "default" {
                return Err("不能删除默认智能体".to_string());
            }
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("删除 Agent 目录失败: {}", e))
        } else {
            Ok(())
        }
    }

    /// 扫描所有 Agent（目录下包含 agent.json 或 SOUL.md 的视为有效 Agent）
    pub fn list_all(paths: &SoulPaths) -> Vec<String> {
        let dir = &paths.agent_default_dir;
        if !dir.exists() {
            return Vec::new();
        }
        let mut agents = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !name.starts_with('.') {
                            // 必须包含 agent.json 或 SOUL.md
                            if path.join("agent.json").exists() || path.join("SOUL.md").exists() {
                                agents.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
        agents.sort();
        agents
    }

    /// 获取 Agent 的 SOUL.md 内容
    pub fn get_soul_content(paths: &SoulPaths, agent_id: &str) -> Result<String, String> {
        let path = paths.soul_path(agent_id);
        if !path.exists() {
            return Err(format!("Agent '{}' 的 SOUL.md 未找到", agent_id));
        }
        std::fs::read_to_string(&path)
            .map_err(|e| format!("读取 SOUL.md 失败: {}", e))
    }

    /// 写入 Agent 的 SOUL.md 内容
    pub fn save_soul_content(paths: &SoulPaths, agent_id: &str, content: &str) -> Result<(), String> {
        let dir = paths.agent_dir(agent_id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建 Agent 目录失败: {}", e))?;
        std::fs::write(paths.soul_path(agent_id), content)
            .map_err(|e| format!("写入 SOUL.md 失败: {}", e))
    }
}
