//! 平台注册中心
//!
//! 管理已注册的 IM 平台适配器实例。
//! 参考 Hermes Agent 的 PlatformRegistry（模块级单例）。

use crate::im::adapter::IMAdapter;
use crate::im::types::PlatformType;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 平台注册中心
///
/// 内部维护 `HashMap<PlatformType, Arc<dyn IMAdapter>>`。
/// 支持注册、查找、状态检查。
pub struct PlatformRegistry {
    adapters: RwLock<HashMap<PlatformType, Arc<dyn IMAdapter>>>,
}

impl PlatformRegistry {
    pub fn new() -> Self {
        Self {
            adapters: RwLock::new(HashMap::new()),
        }
    }

    /// 注册平台适配器（后注册同类型覆盖先注册）
    pub async fn register(&self, adapter: Arc<dyn IMAdapter>) {
        let platform = adapter.platform_type();
        tracing::info!("注册 IM 平台适配器: {}", platform);
        self.adapters.write().await.insert(platform, adapter);
    }

    /// 获取指定平台的适配器
    pub fn get(&self, platform: &PlatformType) -> Option<Arc<dyn IMAdapter>> {
        // 使用 blocking_read 因为 HashMap 操作不涉及 await 点
        // 但在 tokio 上下文中，使用 blocking_read 是安全的
        let guard = self.adapters.blocking_read();
        guard.get(platform).cloned()
    }

    /// 获取所有已注册的平台类型列表
    pub fn platforms(&self) -> Vec<PlatformType> {
        let guard = self.adapters.blocking_read();
        guard.keys().cloned().collect()
    }

    /// 检查指定平台是否已连接
    pub fn is_connected(&self, platform: &PlatformType) -> bool {
        let guard = self.adapters.blocking_read();
        guard
            .get(platform)
            .map(|a| a.is_connected())
            .unwrap_or(false)
    }

    /// 获取适配器数量
    pub fn len(&self) -> usize {
        let guard = self.adapters.blocking_read();
        guard.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
