use crate::agent::cot::CotExtractor;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::config::AppConfig;
use crate::llm::client::LlmClient;
use crate::llm::types::{ChatMessage, ChatRequest, StreamEvent};
use crate::tools::registry::ToolRegistry;
use crate::tools::types::AgentStep;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Agent 运行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub session_id: String,
    pub content: String,
    pub iterations: usize,
    pub messages: Vec<AgentMessage>,
    pub reasoning: Option<String>,
}

/// ReAct Agent 运行时
pub struct AgentRuntime {
    /// 当前 Agent 会话
    session: AgentSession,
    /// LLM 客户端
    llm_client: LlmClient,
    /// 工具注册表
    tool_registry: Arc<ToolRegistry>,
    /// 应用配置
    config: AppConfig,
    /// 最大迭代次数
    max_iterations: usize,
    /// 累积的推理内容
    accumulated_reasoning: String,
}

impl AgentRuntime {
    /// 创建新的 Agent 运行时
    pub fn new(
        session: AgentSession,
        llm_client: LlmClient,
        tool_registry: Arc<ToolRegistry>,
        config: &AppConfig,
    ) -> Self {
        let max_iterations = config.max_iterations;
        Self {
            session,
            llm_client,
            tool_registry,
            config: config.clone(),
            max_iterations,
            accumulated_reasoning: String::new(),
        }
    }

    /// 执行 ReAct 循环并流式输出
    /// 返回完整的 AgentResult
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        step_tx: Option<mpsc::Sender<AgentStep>>,
    ) -> Result<AgentResult, AppError> {
        let mut iterations = 0;
        let mut final_content = String::new();

        // 记录用户消息
        self.session.push_user(user_input);

        // 构建系统提示词
        let system_prompt = self.build_system_prompt();

        // ========== ReAct 主循环 ==========
        loop {
            iterations += 1;

            // 防止死循环
            if iterations > self.max_iterations {
                return Err(AppError::AgentError(format!(
                    "超过最大迭代次数限制 ({})",
                    self.max_iterations
                )));
            }

            tracing::info!("ReAct 迭代 {}/{}", iterations, self.max_iterations);

            // ---- 1. Thought + Action: 调用 LLM ----
            let (assistant_message, reasoning) = self
                .call_llm_with_tools(&system_prompt, &step_tx)
                .await?;

            // 累积推理内容
            if let Some(ref r) = reasoning {
                if !r.is_empty() {
                    if !self.accumulated_reasoning.is_empty() {
                        self.accumulated_reasoning.push('\n');
                    }
                    self.accumulated_reasoning.push_str(r);
                }
            }

            // 提取工具调用
            let tool_calls: Vec<AgentToolCall> = assistant_message
                .tool_calls
                .clone()
                .unwrap_or_default();

            // 保存助手消息
            let mut msg_for_session = assistant_message.clone();
            msg_for_session.reasoning = reasoning.clone();
            self.session.push_message(msg_for_session);

            // ---- 2. 检查是否有工具调用 ----
            if tool_calls.is_empty() {
                // 无工具调用 → 任务完成
                final_content = assistant_message.content.clone();
                break;
            }

            // ---- 3. Observation: 执行工具 ----
            tracing::info!("Agent 请求 {} 个工具调用", tool_calls.len());

            for tc in &tool_calls {
                // 发送 Agent step 事件
                if let Some(ref tx) = step_tx {
                    let _ = tx
                        .send(AgentStep {
                            step_type: "tool_call".to_string(),
                            content: format!("调用工具: {}", tc.name),
                            tool_name: Some(tc.name.clone()),
                            tool_result: None,
                            turn: iterations,
                            max_turns: self.max_iterations,
                        })
                        .await;
                }

                // 解析参数
                let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::Value::Null);

                // 执行工具
                let tool_result = match self.tool_registry.execute(&tc.name, args).await {
                    Ok(result) => {
                        // 截断过长结果
                        let truncated = if result.len() > 8000 {
                            format!(
                                "{}...\n\n[结果已截断，原长度: {} 字符]",
                                &result[..8000],
                                result.len()
                            )
                        } else {
                            result
                        };

                        // 发送结果事件
                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_result".to_string(),
                                    content: format!(
                                        "工具 {} 执行完成 ({})",
                                        tc.name,
                                        if truncated.len() > 100 {
                                            format!("{} 字符", truncated.len())
                                        } else {
                                            "ok".to_string()
                                        }
                                    ),
                                    tool_name: Some(tc.name.clone()),
                                    tool_result: Some(
                                        truncated[..truncated.len().min(500)].to_string(),
                                    ),
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }

                        truncated
                    }
                    Err(e) => {
                        let err_msg = format!("工具执行错误: {}", e);
                        tracing::warn!("{}", err_msg);

                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_error".to_string(),
                                    content: err_msg.clone(),
                                    tool_name: Some(tc.name.clone()),
                                    tool_result: None,
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }

                        err_msg
                    }
                };

                // 推入工具结果消息
                self.session.push_tool_result(&tc.id, &tc.name, &tool_result);
            }

            // 继续 ReAct 循环
        }

        // 添加助手最终响应到前端消息
        let _final_agent_msg = AgentMessage {
            role: "assistant".to_string(),
            content: final_content.clone(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            reasoning: None,
        };

        tracing::info!(
            "ReAct 完成: {} 次迭代, {} 字符输出",
            iterations,
            final_content.len()
        );

        Ok(AgentResult {
            session_id: self.session.id.clone(),
            content: final_content,
            iterations,
            messages: self.session.messages.clone(),
            reasoning: if self.accumulated_reasoning.is_empty() {
                None
            } else {
                Some(self.accumulated_reasoning.clone())
            },
        })
    }

    /// 调用 LLM（带工具和流式输出）
    async fn call_llm_with_tools(
        &self,
        system_prompt: &str,
        step_tx: &Option<mpsc::Sender<AgentStep>>,
    ) -> Result<(AgentMessage, Option<String>), AppError> {
        let tools = self.tool_registry.get_schemas().await;
        let tool_count = tools.len();

        // 构建消息列表
        let mut messages: Vec<ChatMessage> = vec![ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }];

        // 添加历史消息
        for msg in &self.session.messages {
            messages.push(ChatMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                tool_calls: msg.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| crate::llm::types::ToolCall {
                            id: tc.id.clone(),
                            call_type: "function".to_string(),
                            function: crate::llm::types::FunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            },
                        })
                        .collect()
                }),
                tool_call_id: msg.tool_call_id.clone(),
                name: msg.tool_name.clone(),
                reasoning_content: msg.reasoning.clone(),
            });
        }

        // 构建 LLM 工具 Schema
        let llm_tools: Vec<crate::llm::types::ToolDef> = tools
            .iter()
            .map(|t| crate::llm::types::ToolDef {
                def_type: "function".to_string(),
                function: crate::llm::types::FunctionDef {
                    name: t.function.name.clone(),
                    description: t.function.description.clone(),
                    parameters: t.function.parameters.clone(),
                },
            })
            .collect();

        // 发送请求
        let request = ChatRequest {
            model: self.session.model.clone(),
            messages,
            temperature: Some(self.config.temperature),
            stream: true,
            tools: if llm_tools.is_empty() {
                None
            } else {
                Some(llm_tools)
            },
        };

        tracing::info!("发送 LLM 请求 ({} 工具, {} 历史消息)", tool_count, self.session.messages.len());

        let mut event_rx = self.llm_client.chat_stream(&request).await?;

        // 收集流式输出
        let mut full_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<AgentToolCall> = Vec::new();

        while let Some(event) = event_rx.recv().await {
            match event {
                StreamEvent::TextDelta(text) => {
                    full_content.push_str(&text);
                    // 推送文本增量
                    if let Some(ref tx) = step_tx {
                        let _ = tx
                            .send(AgentStep {
                                step_type: "text_chunk".to_string(),
                                content: text,
                                tool_name: None,
                                tool_result: None,
                                turn: 0,
                                max_turns: self.max_iterations,
                            })
                            .await;
                    }
                }
                StreamEvent::ReasoningDelta(reasoning) => {
                    accumulated_reasoning.push_str(&reasoning);
                }
                StreamEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments,
                } => {
                    // 累积工具调用
                    while accumulated_tool_calls.len() <= index {
                        accumulated_tool_calls.push(AgentToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                        });
                    }
                    accumulated_tool_calls[index].id = id;
                    accumulated_tool_calls[index].name = name;
                    accumulated_tool_calls[index].arguments = arguments;
                }
                StreamEvent::Done(_) => {
                    break;
                }
                StreamEvent::Error(err) => {
                    return Err(AppError::LlmError(err));
                }
            }
        }

        // 提取 CoT
        let reasoning = CotExtractor::extract(
            &full_content,
            if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(&accumulated_reasoning)
            },
        );

        // 构建助手消息
        let tool_calls = if accumulated_tool_calls.is_empty() {
            None
        } else {
            Some(accumulated_tool_calls)
        };

        let agent_msg = AgentMessage {
            role: "assistant".to_string(),
            content: full_content,
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            reasoning: None,
        };

        Ok((agent_msg, reasoning))
    }

    /// 构建系统提示词
    fn build_system_prompt(&self) -> String {
        if let Some(ref override_prompt) = self.session.system_prompt_override {
            return override_prompt.clone();
        }

        // 从配置推测操作系统
        let os_name = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };

        crate::agent::prompt::SystemPromptBuilder::new(
            &self.config,
            os_name,
            self.session.workspace.as_deref(),
        )
        .with_skills(vec![])
        .build()
    }

    /// 获取 Agent 会话引用
    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    /// 获取 Agent 会话可变引用
    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }
}
