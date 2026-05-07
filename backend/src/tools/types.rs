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
}
