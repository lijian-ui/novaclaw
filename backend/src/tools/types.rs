use serde::{Deserialize, Serialize};

/// 工具定义（与 LLM 交互的 Schema）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 工具执行结果状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolExecutionStatus {
    Success,
    Failed,
    /// 需要用户确认
    PendingApproval,
    Cancelled,
}

/// 需要确认的操作信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequired {
    /// 操作类型
    pub operation_type: String,
    /// 工具名称
    pub tool_name: String,
    /// 工具参数 (完整 JSON)
    pub arguments: String,
    /// 人类可读的提示信息
    pub message: String,
    /// 受影响的文件列表（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_files: Option<Vec<String>>,
}

/// 工具执行结果
#[derive(Debug, Clone)]
pub enum ToolResult {
    /// 成功执行，返回结果字符串
    Success(String),
    /// 需要用户确认，返回确认信息
    PendingApproval(ApprovalRequired),
}

impl From<String> for ToolResult {
    fn from(s: String) -> Self {
        ToolResult::Success(s)
    }
}

impl From<&str> for ToolResult {
    fn from(s: &str) -> Self {
        ToolResult::Success(s.to_string())
    }
}

/// Agent 步骤信息（推送到前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub step_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    pub turn: usize,
    pub max_turns: usize,
    /// 确认请求（仅当 step_type = "approval_required" 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalRequired>,
    /// 确认 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// 缓存 Token 用量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
}
