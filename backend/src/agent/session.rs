use serde::{Deserialize, Serialize};

/// Agent 会话 - 管理单次对话的完整状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// 会话ID
    pub id: String,
    /// 会话名称
    pub name: String,
    /// 创建工作目录
    pub workspace: Option<String>,
    /// 使用的模型名称
    pub model: String,
    /// 系统提示词（可选覆盖）
    pub system_prompt_override: Option<String>,
    /// 对话消息历史
    pub messages: Vec<AgentMessage>,
    /// 压缩次数
    pub compaction_count: u32,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 总输入 Token 计数
    pub total_input_tokens: u64,
    /// 总输出 Token 计数
    pub total_output_tokens: u64,
}

/// Agent 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// 角色: system / user / assistant / tool
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 工具调用列表（assistant 消息可包含）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AgentToolCall>>,
    /// 工具调用ID（tool 消息用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 工具名称（tool 消息用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// 第一次思考内容（CoT）- 用于前端显示为"思考过程"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_reasoning: Option<String>,
    /// 后续思考内容数组（CoT）- 用于前端显示为"Thought"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasonings: Option<Vec<String>>,
    /// 兼容旧字段：完整的推理内容（已废弃，请使用 first_reasoning 和 reasonings）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Agent 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl AgentSession {
    /// 创建新 Agent 会话
    pub fn new(name: &str, model: &str, workspace: Option<&str>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            workspace: workspace.map(|s| s.to_string()),
            model: model.to_string(),
            system_prompt_override: None,
            messages: Vec::new(),
            compaction_count: 0,
            created_at: now.clone(),
            updated_at: now,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    /// 添加消息到会话历史
    pub fn push_message(&mut self, msg: AgentMessage) {
        self.messages.push(msg);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// 添加用户消息
    pub fn push_user(&mut self, content: &str) {
        self.push_message(AgentMessage {
            role: "user".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            reasonings: None,
            reasoning: None,
        });
    }

    /// 添加助手消息
    pub fn push_assistant(&mut self, content: &str, tool_calls: Option<Vec<AgentToolCall>>) {
        self.push_message(AgentMessage {
            role: "assistant".to_string(),
            content: content.to_string(),
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            reasonings: None,
            reasoning: None,
        });
    }

    /// 添加工具结果消息
    pub fn push_tool_result(&mut self, tool_call_id: &str, tool_name: &str, output: &str) {
        self.push_message(AgentMessage {
            role: "tool".to_string(),
            content: output.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            tool_name: Some(tool_name.to_string()),
            first_reasoning: None,
            reasonings: None,
            reasoning: None,
        });
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 清理旧消息（上下文压缩）
    pub fn compact(&mut self, keep_last: usize) {
        if self.messages.len() <= keep_last + 2 {
            return;
        }

        let total = self.messages.len();
        let to_remove = total - keep_last;

        // 保留前2条（系统上下文）和最后 keep_last 条
        let front: Vec<_> = self.messages.iter().take(2).cloned().collect();
        let back: Vec<_> = self.messages.iter().skip(to_remove + 2).cloned().collect();

        let summary = AgentMessage {
            role: "system".to_string(),
            content: format!("[Context compressed: removed {} historical messages, showing recent conversation content]", to_remove),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            reasonings: None,
            reasoning: None,
        };

        self.messages = front;
        self.messages.push(summary);
        self.messages.extend(back);
        self.compaction_count += 1;
    }
}
