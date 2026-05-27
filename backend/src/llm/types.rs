use serde::{Deserialize, Serialize};

/// OpenAI 兼容的聊天消息
/// content: String "hello" 或数组 [{"type":"text","text":"hello"},{"type":"image_url",...}]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// 工具定义（发送给 LLM 的 schema）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 聊天请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<serde_json::Value>,
    /// extra_body: 提供商特定参数（如 DeepSeek 的 thinking mode、Azure 的 deployment_id 等）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<serde_json::Value>,
}

/// 聊天选择
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    #[serde(default)]
    pub index: i32,
    #[serde(default)]
    pub message: Option<AssistantMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<AssistantDelta>,
}

/// 助手完整消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// 流式增量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// 聊天响应（兼容 OpenAI 及本地服务如 LM Studio/Ollama 的响应格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChatChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    /// 缓存 Token 数量（部分 LLM 支持，如 DeepSeek）
    /// DeepSeek API 返回的字段名为 `prompt_cache_hit_tokens`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<i64>,
    /// DeepSeek 精确前缀缓存命中 Token 数（API 返回字段名）
    #[serde(rename = "prompt_cache_hit_tokens", skip_serializing_if = "Option::is_none")]
    pub prompt_cache_hit_tokens: Option<i64>,
    /// DeepSeek 精确前缀缓存未命中 Token 数（API 返回字段名）
    #[serde(rename = "prompt_cache_miss_tokens", skip_serializing_if = "Option::is_none")]
    pub prompt_cache_miss_tokens: Option<i64>,
}

/// Token 用量信息（内部传递用）
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
}

/// SSE 流式事件
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallDelta {
        index: usize,
        id: String,
        name: String,
        arguments: String,
    },
    /// Token 用量（流结束时由 client 发送，用于写回 session 计数）
    Usage {
        prompt_tokens: u64,
        completion_tokens: u64,
        cached_tokens: u64,
    },
    Done(String),
    Error(String),
}

/// 提取推理内容（CoT）
pub fn extract_reasoning(msg: &AssistantMessage) -> Option<String> {
    // 1. reasoning_content 直接字段
    if let Some(ref r) = msg.reasoning_content {
        if !r.is_empty() {
            return Some(r.clone());
        }
    }

    None
}

/// 从流式增量提取推理内容
pub fn extract_delta_reasoning(delta: &AssistantDelta) -> Option<String> {
    if let Some(ref r) = delta.reasoning_content {
        if !r.is_empty() {
            return Some(r.clone());
        }
    }
    None
}
