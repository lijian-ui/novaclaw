/// 思维链（CoT）提取
/// 参考 hermes-agent 的 4-level unified CoT extraction
pub struct CotExtractor;

impl CotExtractor {
    /// 从助手回复中提取推理内容（兼容旧接口）
    /// 支持多提供商格式的统一抽象：
    /// 1. reasoning_content 字段 (DeepSeek/OpenRouter)
    /// 2. reasoning 字段 (Qwen)
    /// 3. 内联 <think>...</think> 标签 (fallback)
    pub fn extract(content: &str, reasoning_field: Option<&str>) -> Option<String> {
        let parts = Self::extract_multiple(content, reasoning_field);
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }

    /// 从助手回复中提取多个独立的推理内容
    /// 返回一个包含多个独立思考块的 Vec
    /// 用于前端区分"第一次思考"和"后续思考"
    pub fn extract_multiple(content: &str, reasoning_field: Option<&str>) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();

        // Level 1: reasoning_content 字段 (DeepSeek / OpenRouter / Qwen)
        if let Some(r) = reasoning_field {
            if !r.is_empty() {
                // 尝试按 <｜end▁of▁thinking｜> 分隔多个思考内容（DeepSeek R1 格式）
                let chunks: Vec<String> = r
                    .split("<｜end▁of▁thinking｜>")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if chunks.is_empty() {
                    parts.push(r.to_string());
                } else {
                    parts.extend(chunks);
                }
            }
        }

        // Level 2: 内联 <think>...</think> 标签（支持多个块）
        if parts.is_empty() {
            if let Some(thinking_blocks) = Self::extract_inline_thinking_blocks(content) {
                parts.extend(thinking_blocks);
            }
        }

        // Level 3: <｜end▁of▁thinking｜> 分隔符（DeepSeek R1 内联格式，无 reasoning_content 字段时）
        if parts.is_empty() {
            if let Some(thinking) = Self::extract_end_of_thinking_marker(content) {
                parts.push(thinking);
            }
        }

        parts
    }

    /// 提取多个内联 <think>...</think> 标签内容
    fn extract_inline_thinking_blocks(content: &str) -> Option<Vec<String>> {
        let think_start = "<think";
        let think_end = "</think>";

        if !content.contains(think_start) {
            return None;
        }

        let mut blocks: Vec<String> = Vec::new();
        let mut search_pos = 0;

        while let Some(start_tag_pos) = content[search_pos..].find(think_start) {
            let actual_start = search_pos + start_tag_pos;
            // 找到开始标签的结束 '>'
            if let Some(tag_end_pos) = content[actual_start..].find('>') {
                let content_start = actual_start + tag_end_pos + 1;
                // 查找对应的结束标签
                if let Some(end_pos) = content[content_start..].find(think_end) {
                    let block_content = &content[content_start..content_start + end_pos];
                    let trimmed = block_content.trim();
                    if !trimmed.is_empty() {
                        blocks.push(trimmed.to_string());
                    }
                    search_pos = content_start + end_pos + think_end.len();
                } else {
                    // 未找到结束标签，提取从开始到末尾的内容（流式未完成的情况）
                    let block_content = &content[content_start..];
                    let trimmed = block_content.trim();
                    if !trimmed.is_empty() {
                        blocks.push(trimmed.to_string());
                    }
                    break;
                }
            } else {
                break;
            }
        }

        if blocks.is_empty() {
            None
        } else {
            Some(blocks)
        }
    }

    /// 提取 <｜end▁of▁thinking｜> 标记之前的全部内容作为推理（修复：不再截断）
    fn extract_end_of_thinking_marker(content: &str) -> Option<String> {
        let marker = "<｜end▁of▁thinking｜>";
        if let Some(pos) = content.find(marker) {
            let reasoning = content[..pos].trim();
            if !reasoning.is_empty() {
                return Some(reasoning.to_string());
            }
        }
        None
    }

    /// 截断推理内容到指定 Token 数（粗略估算：1 token ≈ 4 字符）
    pub fn truncate(reasoning: &str, max_tokens: usize) -> String {
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
