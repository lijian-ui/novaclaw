//! Soul 管理器
//! 
//! 管理所有 Agent 的 Soul 文件，提供统一的加载和缓存接口

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::soul::loader::{SoulLoader, SoulError};
use crate::soul::models::SoulInfo;

/// Soul 管理器
#[derive(Debug, Clone)]
pub struct SoulManager {
    /// Soul 加载器
    loader: Arc<SoulLoader>,
    /// 缓存的 Soul 信息
    cache: Arc<RwLock<HashMap<String, Arc<SoulInfo>>>>,
    /// 当前选中的 Agent
    current_agent: Arc<RwLock<String>>,
}

impl Default for SoulManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SoulManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self {
            loader: Arc::new(SoulLoader::new()),
            cache: Arc::new(RwLock::new(HashMap::new())),
            current_agent: Arc::new(RwLock::new("default".to_string())),
        }
    }

    /// 获取当前 Agent 的 Soul
    pub async fn get_current_soul(&self) -> Result<SoulInfo, SoulError> {
        let agent_name = self.current_agent.read().await.clone();
        self.get_soul(&agent_name).await
    }

    /// 获取指定 Agent 的 Soul
    pub async fn get_soul(&self, agent_name: &str) -> Result<SoulInfo, SoulError> {
        // 1. 先检查缓存
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(agent_name) {
                tracing::debug!("Soul for agent '{}' loaded from cache", agent_name);
                return Ok((**cached).clone());
            }
        }

        // 2. 从文件系统加载
        let soul_info = self.loader.load(agent_name)?;

        // 3. 写入缓存
        {
            let mut cache = self.cache.write().await;
            cache.insert(agent_name.to_string(), Arc::new(soul_info.clone()));
        }

        tracing::info!("Soul for agent '{}' loaded and cached", agent_name);
        Ok(soul_info)
    }

    /// 设置当前 Agent
    pub async fn set_current_agent(&self, agent_name: String) {
        let mut current = self.current_agent.write().await;
        *current = agent_name.clone();
        tracing::info!("Current agent switched to '{}'", agent_name);
    }

    /// 获取当前 Agent 名称
    pub async fn get_current_agent_name(&self) -> String {
        self.current_agent.read().await.clone()
    }

    /// 列出所有可用的 Agent
    pub fn list_agents(&self) -> Vec<String> {
        self.loader.list_agents()
    }

    /// 清除缓存
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        tracing::info!("Soul cache cleared");
    }

    /// 重新加载指定 Agent 的 Soul
    pub async fn reload(&self, agent_name: &str) -> Result<SoulInfo, SoulError> {
        // 清除缓存
        {
            let mut cache = self.cache.write().await;
            cache.remove(agent_name);
        }

        // 重新加载
        self.get_soul(agent_name).await
    }

    /// 检查指定 Agent 是否存在
    pub fn agent_exists(&self, agent_name: &str) -> bool {
        self.loader.exists(agent_name)
    }
}
