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
    /// 冻结的系统提示词前缀（会话生命周期内只构建一次，用于 DeepSeek 前缀缓存）
    pub frozen_system_prompt: Option<String>,
    /// 对话消息历史（仅追加，除 compact_in_place 外不得原地修改）
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
    /// 缓存命中 Token 数（DeepSeek 精确前缀缓存）
    pub cache_hit_tokens: u64,
    /// 缓存未命中 Token 数
    pub cache_miss_tokens: u64,
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
    pub again_reasonings: Option<Vec<String>>,
    /// 兼容旧字段：完整的推理内容（已废弃，请使用 first_reasoning 和 again_reasonings）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// 图片 data URL 列表（仅 user 消息，临时传递，不持久化到 AgentSession）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
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
            frozen_system_prompt: None,
            messages: Vec::new(),
            compaction_count: 0,
            created_at: now.clone(),
            updated_at: now,
            total_input_tokens: 0,
            total_output_tokens: 0,
            cache_hit_tokens: 0,
            cache_miss_tokens: 0,
        }
    }

    /// 添加消息到会话历史（仅追加！不允许原地修改已有消息）
    pub fn push_message(&mut self, msg: AgentMessage) {
        self.messages.push(msg);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// 设置冻结的系统提示词前缀（会话期间仅调用一次）
    /// 返回 true 表示首次设置，false 表示已存在（重复调用被忽略）
    pub fn set_frozen_system_prompt(&mut self, prompt: String) -> bool {
        if self.frozen_system_prompt.is_some() {
            tracing::debug!("[Cache] frozen_system_prompt 已存在，忽略重复设置");
            return false;
        }
        tracing::info!("[Cache] frozen_system_prompt 已设置 ({} 字符)", prompt.len());
        self.frozen_system_prompt = Some(prompt);
        true
    }

    /// 获取缓存命中率（0.0 ~ 1.0）
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hit_tokens + self.cache_miss_tokens;
        if total == 0 {
            0.0
        } else {
            self.cache_hit_tokens as f64 / total as f64
        }
    }

    /// 添加用户消息
    pub fn push_user(&mut self, content: &str) {
        self.push_user_with_images(content, &[])
    }

    /// 添加用户消息（含图片 data URL）
    pub fn push_user_with_images(&mut self, content: &str, image_urls: &[String]) {
        self.push_message(AgentMessage {
            role: "user".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            images: if image_urls.is_empty() { None } else { Some(image_urls.to_vec()) },
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
            again_reasonings: None,
            reasoning: None,
            images: None,
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
            again_reasonings: None,
            reasoning: None,
            images: None,
        });
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 上下文就地压缩（⚠️ 仅此方法允许修改 messages 数组内容）
    ///
    /// # 缓存影响
    /// - 压缩后消息数组被重写，下一次 LLM 请求的字节前缀与之前不同
    /// - 即：下一次请求会触发缓存未命中（cache miss）
    /// - 但压缩后新的前缀会保持稳定，后续请求可命中新缓存
    /// - 建议：在需要时调用，不要频繁触发
    ///
    /// # 行为
    /// - 保留前 2 条（系统上下文）和最后 `keep_last` 条
    /// - 中间的旧消息被一条摘要消息替换
    /// - `ai_summary` 为 None 时使用简单的计数占位符；为 Some 时使用 LLM 生成的语义摘要
    /// - 摘要消息 role 设置为 `assistant`（更符合对话语境，防止与 system prompt 混淆）
    pub fn compact_in_place(&mut self, keep_last: usize, ai_summary: Option<String>) {
        if self.messages.len() <= keep_last + 2 {
            return;
        }

        let total = self.messages.len();
        let to_remove = total - keep_last;

        // 保留前2条（系统上下文）和最后 keep_last 条
        let front: Vec<_> = self.messages.iter().take(2).cloned().collect();
        let back: Vec<_> = self.messages.iter().skip(to_remove + 2).cloned().collect();

        let summary_content = match ai_summary {
            Some(ref s) if !s.trim().is_empty() => {
                format!("[CONVERSATION HISTORY SUMMARY — earlier turns folded for context efficiency]\n\n{}", s.trim())
            }
            _ => {
                format!("[CONVERSATION HISTORY SUMMARY — removed {} historical messages, showing recent conversation content]", to_remove)
            }
        };

        let summary = AgentMessage {
            role: "assistant".to_string(), // 使用 assistant 角色，保持对话连贯性
            content: summary_content,
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            images: None,
        };

        self.messages = front;
        self.messages.push(summary);
        self.messages.extend(back);
        self.compaction_count += 1;
        if ai_summary.is_some() {
            tracing::info!(
                "[Cache] compact_in_place (AI 摘要): 移除了 {} 条消息，剩余 {} 条 (压缩次数: {})，下次请求会触发缓存未命中",
                to_remove,
                self.messages.len(),
                self.compaction_count
            );
        } else {
            tracing::info!(
                "[Cache] compact_in_place: 移除了 {} 条消息，剩余 {} 条 (压缩次数: {})，下次请求会触发缓存未命中",
                to_remove,
                self.messages.len(),
                self.compaction_count
            );
        }

        // 压缩后清理孤立 tool_calls
        Self::strip_orphan_tool_calls(&mut self.messages);
    }

    /// 扫描并清理孤立 tool_calls — 不完整配对的 tool_calls 会被移除，防止违反 API 协议
    fn strip_orphan_tool_calls(messages: &mut Vec<AgentMessage>) {
        let mut tool_call_ids_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut tool_call_ids_missing: Vec<String> = Vec::new();

        // 第一遍：收集所有 tool_call_ids
        for msg in messages.iter() {
            if msg.role == "tool" {
                if let Some(ref id) = msg.tool_call_id {
                    tool_call_ids_seen.insert(id.clone());
                }
            }
        }

        // 第二遍：找出未匹配的 tool_calls
        for msg in messages.iter() {
            if msg.role == "assistant" {
                if let Some(ref calls) = msg.tool_calls {
                    for tc in calls {
                        if !tool_call_ids_seen.contains(&tc.id) {
                            tool_call_ids_missing.push(tc.id.clone());
                        }
                    }
                }
            }
        }

        if tool_call_ids_missing.is_empty() {
            return;
        }

        // 第三遍：从 assistant 消息中移除缺失的 tool_calls
        for msg in messages.iter_mut() {
            if msg.role != "assistant" {
                continue;
            }
            if let Some(ref mut calls) = msg.tool_calls {
                calls.retain(|tc| !tool_call_ids_missing.contains(&tc.id));
                if calls.is_empty() {
                    msg.tool_calls = None;
                }
            }
        }
    }
}
