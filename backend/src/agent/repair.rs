use super::session::AgentToolCall;

/// Scavenge：从推理内容中提取被 DeepSeek 模型嵌入的工具调用
///
/// ## 为什么需要 Scavenge？
/// DeepSeek V4 全系列模型（v4-flash、v4-pro、R1 等）经常将工具调用
/// 写在 `reasoning_content` 而非标准的 `tool_calls` 字段。
/// 这会导致工具调用被当作推理内容处理，模型收不到工具执行结果。
///
/// ## 支持的提取模式
/// 1. JSON 格式：`{"tool_calls": [{"id":"...", "type":"function", "function":{...}}]}`
/// 2. JSON 数组格式：`[{"id":"...", "type":"function", "function":{...}}]`
/// 3. 内联 JSON 对象：`{"id":"...", "name":"...", "arguments":"..."}`
/// 4. 通过 `response` 标签后的 JSOn（DeepSeek 常见行为）
///
/// ## 参考
/// DeepSeek-Reasonix 的 repair pipeline：scavenge 是四阶段修复的第一步
pub fn scavenge(reasoning_content: &str) -> Vec<AgentToolCall> {
    if reasoning_content.is_empty() {
        return Vec::new();
    }

    let mut extracted: Vec<AgentToolCall> = Vec::new();

    // 模式 1：查找 ` response` 标签后的 JSON 工具调用
    // DeepSeek 经常在推理结束后输出  response\n<tool_calls_json>
    if let Some(after_response) = extract_after_response_tag(reasoning_content) {
        if let Some(calls) = try_extract_json_tool_calls(&after_response) {
            for tc in calls {
                if !extracted.iter().any(|e| e.name == tc.name && e.arguments == tc.arguments) {
                    extracted.push(tc);
                }
            }
        }
    }

    // 模式 2：扫描全文，查找 JSON 工具调用模式
    if let Some(calls) = try_extract_json_tool_calls(reasoning_content) {
        for tc in calls {
            if !extracted.iter().any(|e| e.name == tc.name && e.arguments == tc.arguments) {
                extracted.push(tc);
            }
        }
    }

    // 模式 3：扫描 Markdown 代码块中的 JSON
    extract_json_from_code_blocks(reasoning_content, &mut extracted);

    // 模式 4：扫描独立的 JSON 对象（工具调用扁平结构）
    extract_inline_tool_call_objects(reasoning_content, &mut extracted);

    if !extracted.is_empty() {
        tracing::info!(
            "[Scavenge] 从推理内容中提取了 {} 个工具调用",
            extracted.len()
        );
        for tc in &extracted {
            tracing::debug!(
                "[Scavenge]   → {}({})",
                tc.name,
                if tc.arguments.len() > 80 {
                    format!("{}...", crate::utils::safe_truncate(&tc.arguments, 80))
                } else {
                    tc.arguments.clone()
                }
            );
        }
    }

    extracted
}

/// 提取 ` response` 标签后的内容
fn extract_after_response_tag(content: &str) -> Option<String> {
    // 匹配： response、/response、<response>、</response> 等变体
    let re = regex::Regex::new(r"(?i)(?:</?response\s*>|\\s*response\\s*)").ok()?;
    if let Some(m) = re.find(content) {
        let after = content[m.end()..].trim();
        if !after.is_empty() {
            return Some(after.to_string());
        }
    }
    // 也尝试匹配  think... response 模式
    let re2 = regex::Regex::new(r"(?i)response[\s\n]*:").ok()?;
    if let Some(m) = re2.find(content) {
        let after = content[m.end()..].trim();
        if !after.is_empty() {
            return Some(after.to_string());
        }
    }
    None
}

/// 尝试从字符串中解析 JSON 工具调用
fn try_extract_json_tool_calls(text: &str) -> Option<Vec<AgentToolCall>> {
    // 尝试解析为标准 OpenAI 格式：{"tool_calls": [...]}
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(calls) = extract_from_json_value(&val) {
            if !calls.is_empty() {
                return Some(calls);
            }
        }
    }

    // 尝试查找 JSON 块（内容可能被文字包围）
    let re = regex::Regex::new(r#""tool_calls"\s*:\s*\[[\s\S]*?\]"#).ok()?;
    if let Some(m) = re.find(text) {
        let wrapped = format!("{{{}}}", m.as_str());
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&wrapped) {
            if let Some(calls) = extract_from_json_value(&val) {
                if !calls.is_empty() {
                    return Some(calls);
                }
            }
        }
    }

    None
}

/// 从 JSON Value 中提取工具调用
fn extract_from_json_value(val: &serde_json::Value) -> Option<Vec<AgentToolCall>> {
    // 格式 1: {"tool_calls": [{"id": "...", "function": {"name": "...", "arguments": "..."}}]}
    if let Some(calls) = val.get("tool_calls").and_then(|v| v.as_array()) {
        let extracted: Vec<AgentToolCall> = calls
            .iter()
            .filter_map(|tc| {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                if name.is_empty() {
                    return None;
                }
                Some(AgentToolCall {
                    id: if id.is_empty() {
                        format!("scavenge_{}", uuid::Uuid::new_v4())
                    } else {
                        id.to_string()
                    },
                    name: name.to_string(),
                    arguments: args.to_string(),
                })
            })
            .collect();
        if !extracted.is_empty() {
            return Some(extracted);
        }
    }

    // 格式 2: 直接数组 [{"id": "...", "function": {"name": "...", "arguments": "..."}}]
    if let Some(arr) = val.as_array() {
        let extracted: Vec<AgentToolCall> = arr
            .iter()
            .filter_map(|tc| {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .or_else(|| tc.get("name").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .or_else(|| tc.get("arguments").and_then(|v| v.as_str()))
                    .unwrap_or("{}");
                if name.is_empty() {
                    return None;
                }
                Some(AgentToolCall {
                    id: if id.is_empty() {
                        format!("scavenge_{}", uuid::Uuid::new_v4())
                    } else {
                        id.to_string()
                    },
                    name: name.to_string(),
                    arguments: args.to_string(),
                })
            })
            .collect();
        if !extracted.is_empty() {
            return Some(extracted);
        }
    }

    None
}

/// 从 Markdown 代码块中提取 JSON 工具调用
fn extract_json_from_code_blocks(content: &str, extracted: &mut Vec<AgentToolCall>) {
    let re = regex::Regex::new(r"```(?:json)?\s*\n?([\s\S]*?)```").ok();
    if let Some(re) = re {
        for cap in re.captures_iter(content) {
            if let Some(block) = cap.get(1) {
                let block_text = block.as_str().trim();
                if block_text.contains("tool_calls") || block_text.contains("\"function\"") {
                    if let Some(calls) = try_extract_json_tool_calls(block_text) {
                        for tc in calls {
                            if !extracted.iter().any(|e| e.name == tc.name && e.arguments == tc.arguments) {
                                extracted.push(tc);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 扫描独立的工具调用 JSON 对象（扁平结构）
fn extract_inline_tool_call_objects(content: &str, extracted: &mut Vec<AgentToolCall>) {
    // 匹配形如 {"name": "xxx", "arguments": {...}} 或 {"name": "xxx", "input": {...}} 的独立对象
    let re = regex::Regex::new(r#"\{"name"\s*:\s*"([^"]+)"\s*,\s*"(?:arguments|input)"\s*:\s*(\{[\s\S]*?\})"#).ok();
    if let Some(re) = re {
        for cap in re.captures_iter(content) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let args_str = cap.get(2).map(|m| m.as_str()).unwrap_or("{}").to_string();
            if !name.is_empty() && !extracted.iter().any(|e| e.name == name && e.arguments == args_str) {
                extracted.push(AgentToolCall {
                    id: format!("scavenge_{}", uuid::Uuid::new_v4()),
                    name,
                    arguments: args_str,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scavenge_standard_json() {
        let reasoning = r#"经过分析，我需要调用天气查询工具。
{"tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "get_weather", "arguments": "{\"location\": \"Beijing\"}"}}]}
接下来我会处理结果。"#;
        let calls = scavenge(reasoning);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
    }

    #[test]
    fn test_scavenge_after_response_tag() {
        let reasoning = r#"让我思考一下... step 3 + 5 = 8。
 response
[{"function": {"name": "calculator", "arguments": "{\"a\": 3, \"b\": 5}"}}]"#;
        let calls = scavenge(reasoning);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "calculator");
    }

    #[test]
    fn test_scavenge_code_block() {
        let reasoning = r#"我需要查询用户数据：
```json
{"tool_calls": [{"function": {"name": "query_user", "arguments": "{\"id\": 1}"}}]}
```
这是处理结果。"#;
        let calls = scavenge(reasoning);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "query_user");
    }

    #[test]
    fn test_scavenge_no_tool_calls() {
        let reasoning = r#"这只是普通的推理内容，没有工具调用。"#;
        let calls = scavenge(reasoning);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_scavenge_inline_object() {
        let reasoning = r#"我需要调用工具：{"name": "search_web", "arguments": {"query": "latest news"}}"#;
        let calls = scavenge(reasoning);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_web");
    }
}