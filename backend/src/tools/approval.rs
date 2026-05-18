//! 工具确认管理器
//!
//! 提供三种功能：
//! 1. **`resolve_path`** — 文件路径解析 + 路径穿越防护（不依赖文件存在性）
//! 2. **`execute_delete_file`** — 执行文件/目录删除（含 TOCTOU 重新校验）
//! 3. **`ApprovalManager`** — 待确认操作的注册、查询、移除和超时清理
//!
//! ## 安全设计
//!
//! - 路径防护使用手动 `..` 规范化，不依赖 `canonicalize()`（避免路径不存在时退化）
//! - 删除前重新 canonicalize 已解析路径，防止 TOCTOU 符号链接攻击
//! - 并发确认使用原子 remove-then-execute 模式

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::ApprovalRequired;

/// 手动规范化路径，解析所有 `..` 和 `.` 组件
///
/// 与 `fs::canonicalize()` 不同，此函数**不要求路径存在**，
/// 可对不存在的文件路径进行安全的规范化。
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                // 仅在非根目录时弹出上一级，防止越出根
                if !components.is_empty()
                    && *components.last().unwrap() != Component::ParentDir
                {
                    components.pop();
                }
            }
            Component::CurDir => {
                // 忽略 .
            }
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// 解析文件路径，包含路径穿越防护
///
/// 安全策略（三层防护）：
/// 1. 绝对路径直接返回（信任用户显式指定）
/// 2. 相对路径基于 base_dir（workspace）拼接
/// 3. 手动规范化 + 字符串前缀双重校验
fn resolve_path(path_str: &str, args: &serde_json::Value) -> PathBuf {
    let path = Path::new(path_str);

    if path.is_absolute() {
        return path.to_path_buf();
    }

    // 优先使用注入的 session workspace
    let base_dir = if let Some(ws) = args.get("_workspace").and_then(|v| v.as_str()) {
        PathBuf::from(ws)
    } else {
        crate::config::get_workspace_dir()
    };

    // 拼接后手动规范化（处理所有 .. 和 .）
    let joined = base_dir.join(path);
    let normalized = normalize_path(&joined);

    // 获取规范化后的 base_dir 作为安全边界
    let safe_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| normalize_path(&base_dir));

    let normalized_str = normalized.to_string_lossy().to_string();

    // 校验：规范化后的路径必须位于安全边界内
    // 使用 Path::starts_with() 而非字符串比较，避免 Windows 长路径 \\?\ 前缀导致误判
    if normalized.starts_with(&safe_base) {
        tracing::debug!("[Security] 路径安全: {} → {}", path_str, normalized_str);
        return normalized;
    }

    // 路径越界，阻止
    tracing::warn!(
        "[Security] 路径穿越尝试被阻止: {} → {} (边界: {})",
        path_str,
        normalized_str,
        safe_base.display()
    );
    safe_base
}

/// 执行前对路径做二次安全校验（TOCTOU 防护）
///
/// 在文件删除前重新 canonicalize 路径，确保：
/// 1. 文件没有被符号链接替换
/// 2. 目标文件仍然位于安全边界内
fn validate_path_for_execution(resolved: &Path, base_dir: &Path) -> Result<PathBuf, String> {
    // 获取安全的 base 目录 canonical 路径
    let safe_base = base_dir
        .canonicalize()
        .map_err(|e| format!("安全边界目录不可访问: {}", e))?;

    // 对目标路径做 canonicalize（要求文件存在，防止符号链接攻击）
    let target = resolved
        .canonicalize()
        .map_err(|e| format!("目标路径不可访问: {}", e))?;

    // 检查目标是否在安全边界内
    if !target.starts_with(&safe_base) {
        return Err(format!(
            "安全校验失败：目标路径 {} 不在工作区 {} 内",
            target.display(),
            safe_base.display()
        ));
    }

    Ok(target)
}

/// 执行真正的删除操作（含 TOCTOU 防护）
pub async fn execute_delete_file(
    args: serde_json::Value,
    workspace: Option<&str>,
) -> Result<String, String> {
    let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
    let resolved = if let Some(ws) = workspace {
        let mut args_with_ws = args.clone();
        if let Some(obj) = args_with_ws.as_object_mut() {
            obj.insert(
                "_workspace".to_string(),
                serde_json::Value::String(ws.to_string()),
            );
        }
        resolve_path(path_str, &args_with_ws)
    } else {
        resolve_path(path_str, &args)
    };

    if !resolved.exists() {
        return Err(format!("Path not found: {}", resolved.display()));
    }

    // TOCTOU 防护：删除前重新校验路径安全性
    let base_dir = if let Some(ws) = workspace {
        PathBuf::from(ws)
    } else {
        crate::config::get_workspace_dir()
    };
    let safe_target = validate_path_for_execution(&resolved, &base_dir)?;

    if safe_target.is_dir() {
        std::fs::remove_dir_all(&safe_target)
            .map_err(|e| format!("Failed to delete directory: {}", e))?;
        Ok(format!("Deleted directory: {}", safe_target.display()))
    } else {
        std::fs::remove_file(&safe_target)
            .map_err(|e| format!("Failed to delete file: {}", e))?;
        Ok(format!("Deleted file: {}", safe_target.display()))
    }
}

/// 生成 `delete_file` 的确认消息和受影响的文件列表
pub fn build_delete_file_approval(
    path_str: &str,
    args: &serde_json::Value,
) -> Option<ApprovalRequired> {
    let resolved = resolve_path(path_str, args);

    let affected_files = if resolved.is_dir() {
        // 目录：列出前 20 个条目作为预览
        let mut entries: Vec<String> = Vec::new();
        if let Ok(dir) = std::fs::read_dir(&resolved) {
            for entry in dir.flatten().take(20) {
                entries.push(entry.path().to_string_lossy().to_string());
            }
            if entries.len() >= 20 {
                entries.push("...（更多内容）".to_string());
            }
        }
        if entries.is_empty() {
            entries.push(resolved.to_string_lossy().to_string());
        }
        entries
    } else {
        vec![resolved.to_string_lossy().to_string()]
    };

    let message = if resolved.is_dir() {
        format!(
            "确定要删除目录及其所有内容吗？\n路径: {}\n包含 {} 个子项",
            resolved.display(),
            affected_files.len()
        )
    } else {
        format!(
            "确定要删除文件吗？\n路径: {}",
            resolved.display()
        )
    };

    Some(ApprovalRequired {
        operation_type: "delete_file".to_string(),
        tool_name: "delete_file".to_string(),
        arguments: serde_json::to_string(args).unwrap_or_default(),
        message,
        affected_files: Some(affected_files),
    })
}

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
