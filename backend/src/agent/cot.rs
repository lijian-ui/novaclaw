/// 思维链（CoT）提取
/// 参考 hermes-agent 的 4-level unified CoT extraction
pub struct CotExtractor;

impl CotExtractor {
    /// 从助手回复中提取推理内容
    /// 支持多提供商格式的统一抽象：
    /// 1. reasoning_content 字段 (DeepSeek/OpenRouter)
    /// 2. reasoning 字段 (Qwen)
    /// 3. 内联  thinking 标签 (fallback)
    pub fn extract(content: &str, reasoning_field: Option<&str>) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();

        // Level 1: reasoning_content (DeepSeek / OpenRouter)
        if let Some(r) = reasoning_field {
            if !r.is_empty() {
                parts.push(r.to_string());
            }
        }

        // Level 3: 内联  thinking 标签 (兜底)
        if parts.is_empty() {
            if let Some(thinking) = Self::extract_inline_thinking(content) {
                parts.push(thinking);
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }

    /// 提取内联  thinking 标签内容
    fn extract_inline_thinking(content: &str) -> Option<String> {
        let start_marker = "<｜end▁of▁thinking｜>";
        let _end_marker = "";

        if let Some(start_pos) = content.find(start_marker) {
            let before = &content[..start_pos];
            // 简单提取：保留 response前的最后 2000 字符作为 reasoning
            let start = if before.len() > 2000 {
                before.len() - 2000
            } else {
                0
            };
            let reasoning = &before[start..];
            if !reasoning.trim().is_empty() {
                return Some(reasoning.trim().to_string());
            }
        }
        None
    }

    /// 截断推理内容到指定 Token 数
    pub fn truncate(reasoning: &str, max_tokens: usize) -> String {
        // 粗略估算: 1 token ≈ 4 字符
        let max_chars = max_tokens * 4;
        if reasoning.len() <= max_chars {
            reasoning.to_string()
        } else {
            format!(
                "{}...[推理内容已截断，总字符数: {}]",
                &reasoning[..max_chars.min(reasoning.len())],
                reasoning.len()
            )
        }
    }
}
