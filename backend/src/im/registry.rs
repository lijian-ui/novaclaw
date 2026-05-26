//! 账号注册中心
//!
//! 管理已注册的 IM 平台账号适配器实例。
//! 内部用 `{platform}:{account_id}` 复合 key 避免不同平台同名冲突。

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
/// 内部使用 `{platform}:{account_id}` 作为存储 key，
/// 避免不同平台使用同一 account_id 时发生冲突。
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

    /// 构建设内部复合 key
    fn account_key(account_id: &str, platform: &PlatformType) -> String {
        format!("{}:{}", platform.as_str(), account_id)
    }

    /// 注册账号适配器（内部用复合 key 存储）
    pub async fn register(&self, info: AccountInfo) {
        let key = Self::account_key(&info.account_id, &info.platform);
        tracing::info!(
            "注册 IM 账号: {} (key={}, platform={}, name={:?})",
            info.account_id, key, info.platform, info.name
        );
        let adapter = info.adapter.clone();
        self.accounts.write().await.insert(key.clone(), adapter);
        self.account_info.write().await.insert(key, info);
    }

    /// 通过 account_id + platform 获取适配器（推荐）
    pub async fn get_account(&self, account_id: &str, platform: &PlatformType) -> Option<Arc<dyn IMAdapter>> {
        let key = Self::account_key(account_id, platform);
        self.accounts.read().await.get(&key).cloned()
    }

    /// 通过完整复合 key 获取适配器（适用于调用方已构造好 key 的场景）
    pub async fn get(&self, key: &str) -> Option<Arc<dyn IMAdapter>> {
        self.accounts.read().await.get(key).cloned()
    }

    /// 通过 account_id + platform 获取账号信息
    pub async fn get_info(&self, account_id: &str, platform: &PlatformType) -> Option<AccountInfo> {
        let key = Self::account_key(account_id, platform);
        self.account_info.read().await.get(&key).cloned()
    }

    /// 检查指定账号是否已连接
    pub async fn is_connected(&self, account_id: &str, platform: &PlatformType) -> bool {
        let key = Self::account_key(account_id, platform);
        self.accounts
            .read()
            .await
            .get(&key)
            .map(|a| a.is_connected())
            .unwrap_or(false)
    }

    /// 获取所有已注册的账号 ID（去重）
    pub async fn account_ids(&self) -> Vec<String> {
        let guard = self.account_info.read().await;
        let mut ids: Vec<String> = guard.values().map(|info| info.account_id.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
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
    pub async fn unregister(&self, account_id: &str, platform: &PlatformType) {
        let key = Self::account_key(account_id, platform);
        tracing::info!("注销 IM 账号: {}", key);
        self.accounts.write().await.remove(&key);
        self.account_info.write().await.remove(&key);
    }

    /// 获取适配器总数
    pub async fn len(&self) -> usize {
        self.accounts.read().await.len()
    }

    /// 是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}