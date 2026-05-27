/// DeepSeek V4 聊天模板
///
/// 提供 DeepSeek V4 系列模型的聊天模板格式化功能。
/// 虽然 DeepSeek API 兼容 OpenAI 格式，但在以下场景需要使用此模板：
/// 1. 精确 Token 估算（与 DeepSeek 分词器对齐）
/// 2. DSML（DeepSeek Markup Language）工具调用标记
/// 3. 思考模式（Thinking Mode）的 extra_body 配置
///
/// 参考：DeepSeek-Reasonix 的 tokenizer.ts

use crate::llm::types::ChatMessage;

/// 判断模型是否使用思考模式（DeepSeek V4 全系列均为思考模式）
pub fn is_thinking_mode_model(model: &str) -> bool {
    let m = model.to_lowercase();
    if m.contains("reasoner") {
        return true;
    }
    if m.contains("deepseek") && (m.contains("v4") || m.contains("chat") || m.contains("coder")) {
        return true;
    }
    false
}

/// 获取 DeepSeek 模型的 extra_body.thinking 配置
pub fn thinking_mode_for_model(model: &str) -> Option<&'static str> {
    let m = model.to_lowercase();
    if m == "deepseek-chat" {
        return Some("disabled");
    }
    if m.contains("reasoner") || m.contains("v4") || m.contains("coder") {
        return Some("enabled");
    }
    None
}

/// 格式化 DeepSeek V4 聊天模板（用于非 chat 端点或 Token 估算）
///
/// 将标准消息列表渲染为 DeepSeek V4 专有格式的字符串。
/// 包含 DSML 标记、工具结果合并、BOS/EOS 序列。
///
/// 注意：当前项目通过 OpenAI 兼容的 chat API 调用 DeepSeek，
/// 此函数主要用于 Token 估算和格式参考。
#[allow(unused)]
pub fn format_deepseek_prompt(messages: &[ChatMessage], drop_thinking: bool) -> String {
    let mut result = String::new();
    let mut idx = 0;
    while idx < messages.len() {
        let msg = &messages[idx];
        match msg.role.as_str() {
            "system" => {
                result.push_str(&format!("<｜begin of sentence｜>{}", get_content(msg)));
            }
            "user" => {
                let content = get_content(msg);
                // 检查下一条是否是 tool 消息，如果是则合并
                let mut combined = content;
                let mut j = idx + 1;
                while j < messages.len() && messages[j].role == "tool" {
                    let tool_content = get_content(&messages[j]);
                    combined.push_str(&format!("\n\nTool Result:\n{}", tool_content));
                    j += 1;
                }
                result.push_str(&format!("<｜User｜>{}<｜end of sentence｜>", combined));
                idx = j - 1;
            }
            "assistant" => {
                let content = get_content(msg);
                let reasoning = msg.reasoning_content.as_deref().unwrap_or("");
                let has_thinking = !drop_thinking && !reasoning.is_empty();
                if has_thinking {
                    result.push_str(&format!(
                        "<｜Assistant｜><think>\n{}\n</think>\n{}<｜end of sentence｜>",
                        reasoning, content
                    ));
                } else {
                    result.push_str(&format!("<｜Assistant｜>{}<｜end of sentence｜>", content));
                }
            }
            "tool" => {
                // 独立的 tool 消息（未合并到 user）
                let content = get_content(msg);
                result.push_str(&format!("\n\nTool Result:\n{}", content));
            }
            _ => {
                let content = get_content(msg);
                result.push_str(&format!("<｜{}｜>{}", msg.role, content));
            }
        }
        idx += 1;
    }
    result
}

/// 从 ChatMessage 中提取 content 字符串
fn get_content(msg: &ChatMessage) -> String {
    match &msg.content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            let mut text = String::new();
            for part in arr {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    text.push_str(t);
                }
            }
            text
        }
        _ => msg.content.to_string(),
    }
}

/// 估算 DeepSeek V4 模型的消息 Token 数（粗略，基于字符数）
///
/// 精确 Token 计数需要完整 BPE 分词器（见 P2-4），
/// 此函数提供快速估算用于上下文预检
pub fn estimate_message_tokens(messages: &[ChatMessage], tools_json: Option<&str>) -> u64 {
    let mut total: u64 = 0;
    for msg in messages {
        let content = get_content(msg);
        // 粗略估算：1 token ≈ 4 字节（英文）或 ≈ 2 字节（中文）
        let char_count = content.len() as u64;
        total += char_count / 2 + 10; // +10 为角色/字段开销
        if msg.role == "assistant" {
            if let Some(ref calls) = msg.tool_calls {
                if let Ok(json) = serde_json::to_string(calls) {
                    total += json.len() as u64 / 2;
                }
            }
        }
    }
    if let Some(tools) = tools_json {
        total += tools.len() as u64 / 2;
    }
    total
}

/// 将工具定义渲染为 DSML 格式字符串（用于 Token 估算）
#[allow(unused)]
pub fn render_tools_dsml(tools: &[crate::llm::types::ToolDef]) -> String {
    let mut result = String::from("<tools>\n");
    for tool in tools {
        result.push_str(&format!(
            "  <tool name=\"{}\" description=\"{}\" />\n",
            tool.function.name,
            tool.function.description.replace('"', "\\\"")
        ));
    }
    result.push_str("</tools>");
    result
}

/// 去除 DeepSeek 模型幻觉生成的工具调用标记
pub fn strip_hallucinated_tool_markup(s: &str) -> String {
    let mut out = s.to_string();
    // 移除 DSML 函数调用标记
    let patterns = [
        "<function_calls>",
        "</function_calls>",
        "<|DSML|function_calls>",
        "</|DSML|function_calls>",
    ];
    for p in &patterns {
        out = out.replace(p, "");
    }
    // 移除未闭合的标记
    if let Some(pos) = out.find("<function") {
        out.truncate(pos);
    }
    if let Some(pos) = out.find("<|DSML|") {
        out.truncate(pos);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thinking_mode_detection() {
        assert!(is_thinking_mode_model("deepseek-v4-flash"));
        assert!(is_thinking_mode_model("deepseek-v4-pro"));
        assert!(is_thinking_mode_model("deepseek-reasoner"));
        assert!(is_thinking_mode_model("deepseek-coder"));
        assert!(!is_thinking_mode_model("gpt-4"));
    }

    #[test]
    fn test_strip_hallucinated_markup() {
        let input = "Let me call the function\n<function_calls>\n<invoke name=\"search\">\n</function_calls>\nOk done";
        let result = strip_hallucinated_tool_markup(input);
        assert!(!result.contains("<function_calls>"));
        assert!(!result.contains("</function_calls>"));
    }

    #[test]
    fn test_format_deepseek_prompt() {
        let msgs = vec![
            ChatMessage {
                role: "system".to_string(),
                content: serde_json::Value::String("You are a helpful assistant.".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String("Hello".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
        ];
        let prompt = format_deepseek_prompt(&msgs, false);
        assert!(prompt.contains("begin of sentence"));
        assert!(prompt.contains("User"));
        assert!(prompt.contains("Hello"));
    }
}