//! 工具确认管理器
//!
//! 提供 **`ApprovalManager`** — 待确认操作的注册、查询、移除和超时清理。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::ApprovalRequired;

// ─── ApprovalManager ─────────────────────────────────────────────────

/// 等待确认的操作
#[derive(Debug)]
struct PendingApproval {
    pub approval: ApprovalRequired,
    pub session_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 确认管理器
///
/// 线程安全的待确认操作注册表。支持注册、原子取走和超时清理。
///
/// **并发安全**：使用 `take_pending()` 原子移除并返回，
/// 避免"检查后移除"的竞态条件。
///
/// **超时**：待确认操作超过 `ttl` 时间后会被 `cleanup_expired()` 清理，默认 5 分钟。
#[derive(Debug, Clone)]
pub struct ApprovalManager {
    pending: Arc<RwLock<HashMap<String, PendingApproval>>>,
    /// 待确认操作的过期时间（分钟）
    ttl_minutes: i64,
}

impl ApprovalManager {
    /// 创建默认的确认管理器（5 分钟超时）
    pub fn new() -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            ttl_minutes: 5,
        }
    }

    /// 创建自定义超时的确认管理器
    pub fn with_ttl(ttl_minutes: i64) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            ttl_minutes: ttl_minutes.max(1), // 最少 1 分钟
        }
    }

    /// 记录待确认操作
    pub async fn add_pending(
        &self,
        approval_id: String,
        approval: ApprovalRequired,
        session_id: String,
        tool_name: String,
        arguments: String,
    ) {
        let pending = PendingApproval {
            approval,
            session_id,
            tool_name,
            arguments,
            created_at: chrono::Utc::now(),
        };
        self.pending.write().await.insert(approval_id, pending);
    }

    /// 原子移除并返回待确认信息
    ///
    /// 与 `get_pending_full` + `remove_pending` 分两步不同，
    /// 此方法在写锁内原子完成读取和删除，彻底避免并发重复确认。
    pub async fn take_pending(
        &self,
        approval_id: &str,
    ) -> Option<(ApprovalRequired, String, String, String)> {
        self.pending.write().await.remove(approval_id).map(|p| {
            (
                p.approval,
                p.session_id,
                p.tool_name,
                p.arguments,
            )
        })
    }

    /// 检查是否有待确认（不移除）
    pub async fn get_pending(&self, approval_id: &str) -> Option<ApprovalRequired> {
        self.pending
            .read()
            .await
            .get(approval_id)
            .map(|p| p.approval.clone())
    }

    /// 获取完整的待确认信息（不移除）
    pub async fn get_pending_full(
        &self,
        approval_id: &str,
    ) -> Option<(ApprovalRequired, String, String, String)> {
        self.pending.read().await.get(approval_id).map(|p| {
            (
                p.approval.clone(),
                p.session_id.clone(),
                p.tool_name.clone(),
                p.arguments.clone(),
            )
        })
    }

    /// 移除已处理的确认
    pub async fn remove_pending(&self, approval_id: &str) {
        self.pending.write().await.remove(approval_id);
    }

    /// 清理超时的确认（超过 TTL 分钟）
    pub async fn cleanup_expired(&self) {
        let now = chrono::Utc::now();
        let ttl = self.ttl_minutes;
        let mut pending = self.pending.write().await;
        pending.retain(|_, p| (now - p.created_at) < chrono::Duration::minutes(ttl));
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_simple() {
        let p = normalize_path(Path::new("/a/b/c"));
        assert_eq!(p, PathBuf::from("/a/b/c"));
    }

    #[test]
    fn test_normalize_path_dotdot() {
        let p = normalize_path(Path::new("/a/b/../c"));
        assert_eq!(p, PathBuf::from("/a/c"));
    }

    #[test]
    fn test_normalize_path_double_dotdot() {
        let p = normalize_path(Path::new("/a/b/c/../../d"));
        assert_eq!(p, PathBuf::from("/a/d"));
    }

    #[test]
    fn test_normalize_path_dot() {
        let p = normalize_path(Path::new("/a/./b"));
        assert_eq!(p, PathBuf::from("/a/b"));
    }

    #[test]
    fn test_normalize_path_relative() {
        let p = normalize_path(Path::new("a/b/../c"));
        assert_eq!(p, PathBuf::from("a/c"));
    }
}
