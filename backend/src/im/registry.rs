//! 账号注册中心
//!
//! 管理已注册的 IM 平台账号适配器实例。
//! 替代原有的 PlatformRegistry，按 accountId 管理适配器，
//! 支持同一平台的多个账号。

use crate::im::adapter::IMAdapter;
use crate::im::types::PlatformType;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 账号信息
#[derive(Clone)]
pub struct AccountInfo {
    pub account_id: String,
    pub platform: PlatformType,
    pub adapter: Arc<dyn IMAdapter>,
    pub enabled: bool,
    pub name: Option<String>,
}

impl std::fmt::Debug for AccountInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountInfo")
            .field("account_id", &self.account_id)
            .field("platform", &self.platform)
            .field("enabled", &self.enabled)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// 账号注册中心
///
/// 按 accountId 管理适配器实例，支持同一平台的多个账号。
pub struct AccountRegistry {
    accounts: RwLock<HashMap<String, Arc<dyn IMAdapter>>>,
    account_info: RwLock<HashMap<String, AccountInfo>>,
}

impl AccountRegistry {
    pub fn new() -> Self {
        Self {
            accounts: RwLock::new(HashMap::new()),
            account_info: RwLock::new(HashMap::new()),
        }
    }

    /// 注册账号适配器
    pub async fn register(&self, info: AccountInfo) {
        tracing::info!(
            "注册 IM 账号: {} (platform={}, name={:?})",
            info.account_id, info.platform, info.name
        );
        let adapter = info.adapter.clone();
        self.accounts.write().await.insert(info.account_id.clone(), adapter);
        self.account_info.write().await.insert(info.account_id.clone(), info);
    }

    /// 获取指定账号的适配器
    pub async fn get(&self, account_id: &str) -> Option<Arc<dyn IMAdapter>> {
        self.accounts.read().await.get(account_id).cloned()
    }

    /// 获取指定平台的第一个可用账号
    pub async fn get_first_by_platform(&self, platform: &PlatformType) -> Option<Arc<dyn IMAdapter>> {
        let guard = self.account_info.read().await;
        for info in guard.values() {
            if info.platform == *platform && info.enabled {
                if let Some(adapter) = self.accounts.read().await.get(&info.account_id) {
                    return Some(adapter.clone());
                }
            }
        }
        None
    }

    /// 获取账号信息
    pub async fn get_info(&self, account_id: &str) -> Option<AccountInfo> {
        self.account_info.read().await.get(account_id).cloned()
    }

    /// 检查账号是否已连接
    pub async fn is_connected(&self, account_id: &str) -> bool {
        self.accounts
            .read()
            .await
            .get(account_id)
            .map(|a| a.is_connected())
            .unwrap_or(false)
    }

    /// 获取所有已注册的账号 ID
    pub async fn account_ids(&self) -> Vec<String> {
        self.accounts.read().await.keys().cloned().collect()
    }

    /// 按平台类型获取所有账号
    pub async fn get_by_platform(&self, platform: &PlatformType) -> Vec<AccountInfo> {
        let guard = self.account_info.read().await;
        guard.values()
            .filter(|info| info.platform == *platform)
            .cloned()
            .collect()
    }

    /// 移除账号
    pub async fn unregister(&self, account_id: &str) {
        tracing::info!("注销 IM 账号: {}", account_id);
        self.accounts.write().await.remove(account_id);
        self.account_info.write().await.remove(account_id);
    }

    /// 获取适配器数量
    pub async fn len(&self) -> usize {
        self.accounts.read().await.len()
    }

    /// 是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}
