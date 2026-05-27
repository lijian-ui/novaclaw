/**
 * utils.rs - 通用工具函数
 */

/// 安全截断字符串：取前 max_len 个字符，避免直接字节切片导致 UTF-8 边界 panic。
/// 若字符串长度未超出则返回原字符串的克隆。
pub fn safe_truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        s.chars().take(max_len).collect()
    } else {
        s.to_string()
    }
}

/// 安全截断并追加省略号，仅在实际截断时追加。
pub fn safe_truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    } else {
        s.to_string()
    }
}