use crate::agent::cot::CotExtractor;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::config::AppConfig;
use crate::llm::client::LlmClient;
use crate::llm::types::{ChatMessage, ChatRequest, StreamEvent};
use crate::skills::loader::SkillDef;
use crate::tools::registry::ToolRegistry;
use crate::tools::types::AgentStep;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Agent 运行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub session_id: String,
    pub content: String,
    pub iterations: usize,
    pub messages: Vec<AgentMessage>,
    /// 第一次思考内容（CoT）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_reasoning: Option<String>,
    /// 后续思考内容数组（CoT）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasonings: Option<Vec<String>>,
    /// 兼容旧字段：完整的推理内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// 是否被用户打断取消
    pub cancelled: bool,
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
    /// 是否已有第一次思考
    has_first_reasoning: bool,
    /// 累积的后续思考内容
    accumulated_reasonings: Vec<String>,
    /// 可用技能列表
    skills: Vec<SkillDef>,
}

impl AgentRuntime {
    /// 创建新的 Agent 运行时
    pub fn new(
        session: AgentSession,
        llm_client: LlmClient,
        tool_registry: Arc<ToolRegistry>,
        config: &AppConfig,
        skills: Vec<SkillDef>,
    ) -> Self {
        let max_iterations = config.max_iterations;
        Self {
            session,
            llm_client,
            tool_registry,
            config: config.clone(),
            max_iterations,
            has_first_reasoning: false,
            accumulated_reasonings: Vec::new(),
            skills,
        }
    }

    /// 执行 ReAct 循环并流式输出
    /// 返回完整的 AgentResult
    /// cancel: 可选取消信号，触发时立即停止生成并返回当前结果
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        step_tx: Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
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
            let (assistant_message, reasoning_blocks, cancelled) = self
                .call_llm_with_tools(&system_prompt, &step_tx, cancel.clone())
                .await?;

            // 如果被取消，保留已生成的部分内容
            if cancelled {
                final_content = assistant_message.content.clone();
                break;
            }

            // 处理推理内容（区分第一次和后续思考）
            // 使用 reasoning_blocks 分离多个独立的思考块
            // 注意：assistant_message 在循环开始时被创建，这里需要重新创建
            let mut msg_for_session = assistant_message.clone();
            msg_for_session.first_reasoning = None;
            msg_for_session.reasonings = None;
            msg_for_session.reasoning = None;

            if !reasoning_blocks.is_empty() {
                if !self.has_first_reasoning {
                    // 第一次思考：第一个块保存到 first_reasoning
                    msg_for_session.first_reasoning = Some(reasoning_blocks[0].clone());
                    // 后续块保存到 reasonings
                    if reasoning_blocks.len() > 1 {
                        msg_for_session.reasonings = Some(reasoning_blocks[1..].to_vec());
                    }
                    self.has_first_reasoning = true;
                } else {
                    // 后续思考：所有块累积到 reasonings
                    let mut all_reasonings = self.accumulated_reasonings.clone();
                    all_reasonings.extend(reasoning_blocks);
                    self.accumulated_reasonings = all_reasonings;
                    msg_for_session.reasonings = Some(self.accumulated_reasonings.clone());
                }
            } else if self.has_first_reasoning && !self.accumulated_reasonings.is_empty() {
                // 继续累积后续的思考内容
                msg_for_session.reasonings = Some(self.accumulated_reasonings.clone());
            }

            // 提取工具调用
            let tool_calls: Vec<AgentToolCall> = assistant_message
                .tool_calls
                .clone()
                .unwrap_or_default();

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
            first_reasoning: None,
            reasonings: None,
            reasoning: None,
        };

        tracing::info!(
            "ReAct 完成: {} 次迭代, {} 字符输出",
            iterations,
            final_content.len()
        );

        // 计算第一次思考（从第一条 assistant 消息获取）
        let first_reasoning = self.session.messages.iter()
            .find(|m| m.role == "assistant" && m.first_reasoning.is_some())
            .and_then(|m| m.first_reasoning.clone());

        let cancelled = cancel.map_or(false, |c| c.load(std::sync::atomic::Ordering::Relaxed));
        Ok(AgentResult {
            session_id: self.session.id.clone(),
            content: final_content,
            iterations,
            messages: self.session.messages.clone(),
            first_reasoning,
            reasonings: if self.accumulated_reasonings.is_empty() {
                None
            } else {
                Some(self.accumulated_reasonings.clone())
            },
            reasoning: None, // 兼容旧字段
            cancelled,
        })
    }

    /// 调用 LLM（带工具和流式输出）
    async fn call_llm_with_tools(
        &self,
        system_prompt: &str,
        step_tx: &Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<(AgentMessage, Vec<String>, bool), AppError> {
        // cancelled 标记由调用方检查，避免外层 ReAct 循环继续
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
            // 合并 first_reasoning 和 reasonings 为完整的 reasoning_content
            let mut all_reasonings = Vec::new();
            if let Some(ref fr) = msg.first_reasoning {
                all_reasonings.push(fr.clone());
            }
            if let Some(ref rs) = msg.reasonings {
                all_reasonings.extend(rs.clone());
            }
            // 兼容旧字段
            if let Some(ref r) = msg.reasoning {
                if !all_reasonings.contains(r) {
                    all_reasonings.push(r.clone());
                }
            }
            let reasoning_content = if all_reasonings.is_empty() {
                None
            } else {
                Some(all_reasonings.join("\n"))
            };

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
                reasoning_content,
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

        let cancel_flag = cancel.clone();
        let mut event_rx = self.llm_client.chat_stream(&request, cancel.clone()).await?;

        // 收集流式输出
        let mut full_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut was_cancelled = false;
        // 追踪已发送 tool_call step 事件的索引，避免重复发送
        let mut sent_tool_call_indices: HashSet<usize> = HashSet::new();

        while let Some(event) = event_rx.recv().await {
            // 取消检查
            if let Some(ref flag) = cancel_flag {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    was_cancelled = true;
                    break;
                }
            }
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
                    // 推送推理内容增量到前端，让用户实时看到思考过程
                    if let Some(ref tx) = step_tx {
                        let _ = tx
                            .send(AgentStep {
                                step_type: "reasoning".to_string(),
                                content: reasoning,
                                tool_name: None,
                                tool_result: None,
                                turn: 0,
                                max_turns: self.max_iterations,
                            })
                            .await;
                    }
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
                    accumulated_tool_calls[index].id = id.clone();
                    accumulated_tool_calls[index].name = name.clone();
                    accumulated_tool_calls[index].arguments = arguments;

                    // 尽早发送 tool_call step 事件：当工具名称已知且尚未报告过时，立即推送到前端
                    if !name.is_empty() && !sent_tool_call_indices.contains(&index) {
                        sent_tool_call_indices.insert(index);
                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_call".to_string(),
                                    content: format!("调用工具: {}", name),
                                    tool_name: Some(name.clone()),
                                    tool_result: None,
                                    turn: 0,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }
                    }
                }
                StreamEvent::Done(_) => {
                    break;
                }
                StreamEvent::Error(err) => {
                    return Err(AppError::LlmError(err));
                }
            }
        }

        // 提取 CoT - 使用 extract_multiple 分离多个独立的思考块
        let reasoning_blocks = CotExtractor::extract_multiple(
            &full_content,
            if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(&accumulated_reasoning)
            },
        );

        // 从 content 中剥离思考标签，避免数据重复保存在 content 和 first_reasoning/reasonings 中
        // 只要已成功提取到思考块（reasoning_blocks 不为空），就始终剥离 Think 标签
        let cleaned_content = if reasoning_blocks.is_empty() {
            // 无独立思考内容，保留原始 content
            full_content.clone()
        } else {
            // 移除完整的 <think>...</think> 块
            let re = regex::Regex::new(r"(?is)<think\s*>[\s\S]*?</think\s*>").unwrap();
            let stripped = re.replace_all(&full_content, "").trim().to_string();
            // 思考内容已通过 first_reasoning/reasonings 字段保存，content 中不再需要 Think 标签
            // 即使剥离后为空（全部内容都是思考），也应返回空字符串避免重复
            stripped
        };

        // 构建助手消息
        let tool_calls = if accumulated_tool_calls.is_empty() {
            None
        } else {
            Some(accumulated_tool_calls)
        };

        let agent_msg = AgentMessage {
            role: "assistant".to_string(),
            content: cleaned_content,
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            reasonings: None,
            reasoning: None,
        };

        Ok((agent_msg, reasoning_blocks, was_cancelled))
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
        .with_skills(self.skills.iter().map(|s| {
            format!("{}: {}", s.name, s.description)
        }).collect())
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
