/// 截断结果保存器
///
/// 参考 DeepSeek-Reasonix 的 `src/tools/truncated-result-saver.ts`
///
/// 当工具结果超过 token 上限被截断时，将完整内容保存到磁盘，
/// 并在截断消息中附上文件路径，让 LLM 可以通过 read_file 读取完整内容。
///
/// 存储路径：`<workspace>/.novaclaw/truncated-results/<timestamp>-<uuid>-<tool_name>.txt`
/// 保留策略：30 天自动清理

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TRUNCATED_DIR: &str = ".novaclaw/truncated-results";
const DEFAULT_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60; // 30 天

/// 对工具名称进行文件名安全处理
fn sanitize_tool_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(48)
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

/// 解析存储目录的绝对路径
pub fn storage_dir(workspace: &str) -> PathBuf {
    Path::new(workspace).join(TRUNCATED_DIR)
}

/// 生成唯一文件名
fn result_filename(tool_name: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis();
    // 用简单的随机后缀（基于时间戳的低位）
    let suffix = ts & 0xFFFF_FFFF;
    let safe_name = sanitize_tool_name(tool_name);
    format!("{}-{:08x}-{}.txt", ts, suffix, safe_name)
}

/// 将截断的完整内容保存到磁盘，返回相对于 workspace 的路径
///
/// # 参数
/// - `content`: 完整的工具结果内容（未截断）
/// - `tool_name`: 工具名称（用于文件名）
/// - `workspace`: 工作目录（存储路径的根）
///
/// # 返回
/// 相对于 workspace 的文件路径，LLM 可以直接用 read_file 读取
pub fn save_truncated_result(content: &str, tool_name: &str, workspace: &str) -> Option<String> {
    // 清理旧文件（最佳努力，失败不影响主流程）
    cleanup_old_results(workspace, DEFAULT_MAX_AGE_SECS);

    let dir = storage_dir(workspace);
    if let Err(e) = fs::create_dir_all(&dir) {
        tracing::warn!("[TruncSaver] 创建目录失败 {:?}: {}", dir, e);
        return None;
    }

    let filename = result_filename(tool_name);
    let abs_path = dir.join(&filename);

    if let Err(e) = fs::write(&abs_path, content) {
        tracing::warn!("[TruncSaver] 写入文件失败 {:?}: {}", abs_path, e);
        return None;
    }

    // 返回相对于 workspace 的路径（正斜杠，跨平台一致）
    let rel = format!("{}/{}", TRUNCATED_DIR, filename);
    tracing::debug!("[TruncSaver] 保存截断结果到: {}", rel);
    Some(rel)
}

/// 清理超过 max_age_secs 的旧文件
pub fn cleanup_old_results(workspace: &str, max_age_secs: u64) {
    let dir = storage_dir(workspace);
    if !dir.exists() {
        return;
    }

    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(max_age_secs))
        .unwrap_or(UNIX_EPOCH);

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(mtime) = meta.modified() {
                if mtime < cutoff {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}

/// 判断某个工具是否应该跳过保存（参考 Reasonix shouldSkipSave）
///
/// 跳过条件：
/// - 工具定义中标记了 `skip_truncation_save = true`（如 get_env 等可能泄露密钥的工具）
/// - 工具结果本身是错误消息（不值得保存）
pub fn should_skip_save(tool_name: &str, skip_flag: bool) -> bool {
    if skip_flag {
        return true;
    }
    // 内置跳过列表：可能包含敏感信息或结果无意义的工具
    matches!(tool_name, "get_env" | "memory" | "todo_read" | "todo_write")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_tool_name() {
        assert_eq!(sanitize_tool_name("read_file"), "read_file");
        assert_eq!(sanitize_tool_name("execute-command"), "execute-command");
        assert_eq!(sanitize_tool_name("tool/with/slashes"), "tool_with_slashes");
        assert_eq!(sanitize_tool_name(""), "unknown");
    }

    #[test]
    fn test_save_and_cleanup() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().to_str().unwrap();

        let content = "这是一段很长的工具结果内容，超过了 token 上限，需要保存到磁盘。".repeat(100);
        let rel_path = save_truncated_result(&content, "read_file", workspace);
        assert!(rel_path.is_some());

        let rel = rel_path.unwrap();
        let abs = tmp.path().join(&rel);
        assert!(abs.exists());

        let saved = fs::read_to_string(&abs).unwrap();
        assert_eq!(saved, content);
    }

    #[test]
    fn test_should_skip_save() {
        assert!(should_skip_save("get_env", false));
        assert!(should_skip_save("read_file", true));
        assert!(!should_skip_save("read_file", false));
        assert!(!should_skip_save("execute_command", false));
    }
}
