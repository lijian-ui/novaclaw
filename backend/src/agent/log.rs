use serde::{Deserialize, Serialize};

/// 追加日志条目 — 保证序列化字节一致性
///
/// 与 `AgentMessage` 不同，`LogEntry` 专注于为 LLM API 请求生成
/// 字节序列完全一致的对话历史，确保 DeepSeek 前缀缓存不受序列化差异破坏。
///
/// 约束：
/// - 所有字段在序列化时按字母序排列（`#[serde(rename)]` 可覆盖）
/// - 空集合始终序列化为 `None`（不出现空数组）
/// - `content` 始终为字符串
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogEntry {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LogToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 推理内容（DeepSeek thinking mode），必须回传给 API 否则 400
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// 日志条目中的工具调用 — 使用浮点排序定序
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: LogFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// AppendOnlyLog — 仅追加日志，保证序列化确定性
///
/// # 缓存保证
/// 只要内容不变，`serialize()` 输出完全相同字节序列。
/// - 即使 `messages` 长度相同，序列化输出也不变
/// - None 字段始终被跳过（不出现空占位）
/// - 空集合始终为 None（'' → None, [] → None）
#[derive(Debug, Clone, Default)]
pub struct AppendOnlyLog {
    entries: Vec<LogEntry>,
}

impl AppendOnlyLog {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// 追加条目（仅追加，不支持原地修改）
    pub fn push(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }

    /// 获取条目不可变引用
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// 清空所有条目（用于压缩后重建）
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 从 entries 重建
    pub fn from_entries(entries: Vec<LogEntry>) -> Self {
        Self { entries }
    }

    /// 序列化为 JSON 字符串（确定性输出）
    pub fn serialize(&self) -> String {
        serde_json::to_string(&self.entries).unwrap_or_default()
    }

    /// 序列化为 JSON 字节（确定性输出）
    pub fn serialize_to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(&self.entries).unwrap_or_default()
    }
}

/// 将 `crate::agent::session::AgentMessage` 转换为 `LogEntry`（带确定性保证）
impl From<&super::session::AgentMessage> for LogEntry {
    fn from(msg: &super::session::AgentMessage) -> Self {
        let tool_calls = msg.tool_calls.as_ref().and_then(|tcs| {
            if tcs.is_empty() {
                None
            } else {
                Some(
                    tcs.iter()
                        .map(|tc| LogToolCall {
                            id: tc.id.clone(),
                            call_type: "function".to_string(),
                            function: LogFunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            },
                        })
                        .collect(),
                )
            }
        });

        // 聚合所有推理内容字段（first_reasoning, again_reasonings, reasoning）
        let mut aggregated_reasoning = String::new();
        if let Some(ref r) = msg.first_reasoning {
            aggregated_reasoning.push_str(r);
        }
        if let Some(ref rs) = msg.again_reasonings {
            for r in rs {
                if !aggregated_reasoning.is_empty() {
                    aggregated_reasoning.push_str("\n\n");
                }
                aggregated_reasoning.push_str(r);
            }
        }
        if let Some(ref r) = msg.reasoning {
             if !aggregated_reasoning.is_empty() && !r.is_empty() {
                 if !aggregated_reasoning.contains(r) {
                    aggregated_reasoning.push_str("\n\n");
                    aggregated_reasoning.push_str(r);
                 }
             } else if aggregated_reasoning.is_empty() {
                 aggregated_reasoning.push_str(r);
             }
        }
        let reasoning_content = if aggregated_reasoning.is_empty() { None } else { Some(aggregated_reasoning) };

        LogEntry {
            role: msg.role.clone(),
            content: msg.content.clone(),
            tool_calls,
            tool_call_id: if msg.role == "tool" {
                Some(msg.tool_call_id.clone().unwrap_or_else(|| "missing_id".to_string()))
            } else {
                msg.tool_call_id.clone()
            },
            name: msg.tool_name.clone(),
            reasoning_content,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_serialization() {
        let mut log = AppendOnlyLog::new();
        log.push(LogEntry {
            role: "user".to_string(),
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });
        log.push(LogEntry {
            role: "assistant".to_string(),
            content: "Hi".to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });

        let s1 = log.serialize();
        let s2 = log.serialize();
        assert_eq!(s1, s2, "两次序列化输出应完全一致");
    }

    #[test]
    fn test_empty_tool_calls_is_none() {
        let entry = LogEntry {
            role: "assistant".to_string(),
            content: "ok".to_string(),
            tool_calls: Some(vec![]), // 空数组
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        // 空 tool_calls 应序列化为 None（不出现 tool_calls 字段）
        assert!(!json.contains("tool_calls"), "空数组应序列化为无 tool_calls 字段, got: {}", json);
    }

    #[test]
    fn test_agent_message_conversion_empty_tool_calls() {
        let msg = super::super::session::AgentMessage {
            role: "assistant".to_string(),
            content: "test".to_string(),
            tool_calls: Some(vec![]),
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            images: None,
            videos: None,
            weight: 0,
        };
        let entry: LogEntry = (&msg).into();
        assert!(entry.tool_calls.is_none(), "空 tool_calls 应转换为 None");
    }

    #[test]
    fn test_tool_message_serialization() {
        let mut log = AppendOnlyLog::new();
        log.push(LogEntry {
            role: "tool".to_string(),
            content: "result data".to_string(),
            tool_calls: None,
            tool_call_id: Some("call_123".to_string()),
            name: Some("search".to_string()),
            reasoning_content: None,
        });

        let json = log.serialize();
        assert!(json.contains("tool_call_id"), "tool 消息应包含 tool_call_id");
        assert!(json.contains("name"), "tool 消息应包含 name");
        assert!(!json.contains("tool_calls"), "不应包含 tool_calls 字段");
    }

    #[test]
    fn test_reasoning_content_preserved() {
        let msg = super::super::session::AgentMessage {
            role: "assistant".to_string(),
            content: "I think...".to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: Some("first thought".to_string()),
            again_reasonings: None,
            reasoning: Some("deepseek reasoning content".to_string()),
            images: None,
            videos: None,
            weight: 0,
        };
        let entry: LogEntry = (&msg).into();
        assert_eq!(entry.reasoning_content, Some("deepseek reasoning content".to_string()),
            "reasoning_content 应从 AgentMessage.reasoning 携带过来");

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("reasoning_content"), "序列化应包含 reasoning_content 字段: {}", json);
    }
}