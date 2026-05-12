use crate::agent::cot::CotExtractor;
use crate::agent::prompt::TaskDecompositionParser;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::agent::task::{TaskPlan, TaskStatus, TaskProgress};
use crate::agent::task_detector::{TaskComplexityDetector, DetectionResult};
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
use std::time::Duration;
use tokio::sync::mpsc;

const COMPACT_THRESHOLD: usize = 40;
const COMPACT_KEEP_LAST: usize = 20;

/// 格式化工具调用显示信息
/// 将 JSON 参数转换为易读的格式，特别是文件类工具显示相对路径和文件名
fn format_tool_call_display(tool_name: &str, arguments: &str) -> String {
    // 尝试解析 JSON 参数
    if let Ok(args) = serde_json::from_str::<serde_json::Value>(arguments) {
        // 提取关键参数
        let file_path = args.get("file_path")
            .or_else(|| args.get("path"))
            .or_else(|| args.get("file"))
            .or_else(|| args.get("filepath"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let rel_path = args.get("rel_path")
            .or_else(|| args.get("relative_path"))
            .or_else(|| args.get("relativePath"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        // 对于文件类工具，格式化显示
        let display_path = rel_path.as_ref().or(file_path.as_ref());
        
        if let Some(path) = display_path {
            // 提取文件名
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path);
            
            // 如果路径包含目录，显示 "相对路径/文件名"
            if let Some(parent) = std::path::Path::new(path).parent() {
                let parent_str = parent.to_string_lossy();
                if !parent_str.is_empty() && parent_str != "." {
                    return format!("{}: {}/{}", tool_name, parent_str, file_name);
                }
            }
            
            return format!("{}: {}", tool_name, file_name);
        }
        
        // 尝试提取其他常见参数
        if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
            return format!("{}: {}", tool_name, content);
        }
        
        if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
            return format!("{}: {}", tool_name, text);
        }
        
        if let Some(query) = args.get("query").and_then(|v| v.as_str()) {
            return format!("{}: {}", tool_name, query);
        }
        
        if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
            return format!("{}: {}", tool_name, cmd);
        }
    }
    
    // 如果无法解析或没有关键参数，返回原始参数（截断）
    if arguments.len() > 200 {
        format!("{}: {}...", tool_name, &arguments[..200])
    } else if arguments.is_empty() || arguments == "{}" {
        tool_name.to_string()
    } else {
        format!("{}: {}", tool_name, arguments)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub session_id: String,
    pub content: String,
    pub iterations: usize,
    pub messages: Vec<AgentMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasonings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub cancelled: bool,
    pub max_iterations_reached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_plan: Option<TaskPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_progress: Option<TaskProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_result: Option<DetectionResult>,
}

pub struct AgentRuntime {
    session: AgentSession,
    llm_client: LlmClient,
    tool_registry: Arc<ToolRegistry>,
    config: AppConfig,
    max_iterations: usize,
    max_retries: u32,
    has_first_reasoning: bool,
    accumulated_reasonings: Vec<String>,
    skills: Vec<SkillDef>,
    task_plan: Option<TaskPlan>,
    completed_tasks: HashSet<String>,
    executed_tools: HashSet<String>,
    detection_result: Option<DetectionResult>,
}

impl AgentRuntime {
    pub fn new(
        session: AgentSession,
        llm_client: LlmClient,
        tool_registry: Arc<ToolRegistry>,
        config: &AppConfig,
        skills: Vec<SkillDef>,
    ) -> Self {
        let max_iterations = config.max_iterations;
        let max_retries = config.max_retries;
        Self {
            session,
            llm_client,
            tool_registry,
            config: config.clone(),
            max_iterations,
            max_retries,
            has_first_reasoning: false,
            accumulated_reasonings: Vec::new(),
            skills,
            task_plan: None,
            completed_tasks: HashSet::new(),
            executed_tools: HashSet::new(),
            detection_result: None,
        }
    }

    pub async fn run_turn(
        &mut self,
        user_input: &str,
        step_tx: Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<AgentResult, AppError> {
        let mut iterations = 0;
        let mut final_content = String::new();
        let mut max_iterations_reached = false;
        let mut is_first_iteration = true;

        self.session.push_user(user_input);

        // ---- 复杂任务预判断 ----
        // 在发送 LLM 请求前，对用户输入进行关键词分析，
        // 判断当前任务是否需要启用任务分解流程
        let detection = TaskComplexityDetector::analyze(user_input);
        self.detection_result = Some(detection.clone());
        if detection.is_complex {
            tracing::info!(
                "[Agent] 检测到复杂任务，评分: {:.2}，匹配 {} 个关键词",
                detection.complexity_score,
                detection.total_matches
            );
            // 向前端推送检测结果
            if let Some(ref tx) = step_tx {
                let _ = tx
                    .send(AgentStep {
                        step_type: "task_detection".to_string(),
                        content: serde_json::to_string(&detection).unwrap_or_default(),
                        tool_name: None,
                        tool_result: None,
                        turn: 0,
                        max_turns: self.max_iterations,
                    })
                    .await;
            }
        } else {
            tracing::debug!(
                "[Agent] 简单任务，评分: {:.2}",
                detection.complexity_score
            );
        }

        if self.session.message_count() > COMPACT_THRESHOLD {
            tracing::info!(
                "[Agent] 消息数 {} 超过阈值 {}，触发上下文压缩，保留最近 {} 条",
                self.session.message_count(),
                COMPACT_THRESHOLD,
                COMPACT_KEEP_LAST
            );
            self.session.compact(COMPACT_KEEP_LAST);
            tracing::info!(
                "[Agent] 压缩完成，当前消息数: {}，累计压缩次数: {}",
                self.session.message_count(),
                self.session.compaction_count
            );
        }

        let system_prompt = self.build_system_prompt();

        loop {
            iterations += 1;

            if iterations > self.max_iterations {
                tracing::warn!(
                    "[Agent] 达到最大迭代次数 {}，优雅停止并返回当前最佳结果",
                    self.max_iterations
                );
                max_iterations_reached = true;
                if final_content.is_empty() {
                    final_content = format!(
                        "[Agent 已达最大迭代次数 {}，任务可能未完全完成，请尝试继续对话]",
                        self.max_iterations
                    );
                }
                break;
            }

            tracing::info!("[Agent] ReAct 迭代 {}/{}", iterations, self.max_iterations);

            let (assistant_message, reasoning_blocks, cancelled) = self
                .call_llm_with_tools_and_retry(&system_prompt, &step_tx, cancel.clone())
                .await?;

            if cancelled {
                final_content = assistant_message.content.clone();
                break;
            }

            if is_first_iteration {
                self.try_parse_task_plan(&assistant_message.content, &step_tx).await;
                is_first_iteration = false;
            }

            let mut msg_for_session = assistant_message.clone();
            msg_for_session.first_reasoning = None;
            msg_for_session.reasonings = None;
            msg_for_session.reasoning = None;

            if !reasoning_blocks.is_empty() {
                // 向前端发送区分类型的思考消息
                if let Some(ref tx) = step_tx {
                    if !self.has_first_reasoning {
                        // 首次思考：发送 first_thought 类型
                        for (idx, block) in reasoning_blocks.iter().enumerate() {
                            let step_type = if idx == 0 { "first_thought" } else { "thought" };
                            let _ = tx
                                .send(AgentStep {
                                    step_type: step_type.to_string(),
                                    content: block.clone(),
                                    tool_name: None,
                                    tool_result: None,
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }
                        self.has_first_reasoning = true;
                    } else {
                        // 后续思考：发送 thought 类型
                        for block in &reasoning_blocks {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "thought".to_string(),
                                    content: block.clone(),
                                    tool_name: None,
                                    tool_result: None,
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }
                        // 累积所有思考内容
                        self.accumulated_reasonings.extend(reasoning_blocks.clone());
                    }
                } else {
                    // 不发送时，仅内部累积
                    if !self.has_first_reasoning {
                        self.has_first_reasoning = true;
                    }
                    self.accumulated_reasonings.extend(reasoning_blocks);
                }
            } else if self.has_first_reasoning && !self.accumulated_reasonings.is_empty() {
                // 有历史思考但当前没有新思考，也需要更新 turn 信息
                if let Some(ref tx) = step_tx {
                    let _ = tx
                        .send(AgentStep {
                            step_type: "thought".to_string(),
                            content: format!("[第 {} 轮思考完成]", iterations),
                            tool_name: None,
                            tool_result: None,
                            turn: iterations,
                            max_turns: self.max_iterations,
                        })
                        .await;
                }
            }

            let tool_calls: Vec<AgentToolCall> = assistant_message
                .tool_calls
                .clone()
                .unwrap_or_default();

            self.session.push_message(msg_for_session);

            if tool_calls.is_empty() {
                final_content = assistant_message.content.clone();
                break;
            }

            let valid_tool_calls = self.filter_duplicate_tool_calls(&tool_calls);
            
            if valid_tool_calls.is_empty() {
                tracing::info!("[Agent] 所有工具调用已执行过，跳过重复执行");
                if let Some(ref tx) = step_tx {
                    let _ = tx
                        .send(AgentStep {
                            step_type: "skip".to_string(),
                            content: "跳过重复工具调用".to_string(),
                            tool_name: None,
                            tool_result: None,
                            turn: iterations,
                            max_turns: self.max_iterations,
                        })
                        .await;
                }
                continue;
            }

            tracing::info!("[Agent] 并发执行 {} 个工具调用", valid_tool_calls.len());

            let tool_futures: Vec<_> = valid_tool_calls.iter().map(|tc| {
                let registry = self.tool_registry.clone();
                let name = tc.name.clone();
                let id = tc.id.clone();
                let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::Value::Null);
                async move {
                    let result = registry.execute(&name, args).await;
                    (id, name, result)
                }
            }).collect();

            let tool_results = futures::future::join_all(tool_futures).await;

            for (tc_id, tc_name, result) in tool_results {
                self.executed_tools.insert(format!("{}_{}", tc_name, tc_id));

                let tool_result = match result {
                    Ok(output) => {
                        let truncated = if output.len() > 8000 {
                            format!(
                                "{}...\n\n[结果已截断，原长度: {} 字符]",
                                &output[..8000],
                                output.len()
                            )
                        } else {
                            output
                        };

                        tracing::info!("[Agent] 工具 {} 执行成功，结果 {} 字符", tc_name, truncated.len());

                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_result".to_string(),
                                    content: format!(
                                        "工具 {} 执行完成 ({})",
                                        tc_name,
                                        if truncated.len() > 100 {
                                            format!("{} 字符", truncated.len())
                                        } else {
                                            "ok".to_string()
                                        }
                                    ),
                                    tool_name: Some(tc_name.clone()),
                                    tool_result: Some(
                                        truncated[..truncated.len().min(500)].to_string(),
                                    ),
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }

                        self.update_task_progress(&tc_name, &truncated, true, None);
                        self.send_task_progress(&step_tx).await;

                        truncated
                    }
                    Err(e) => {
                        let err_msg = format!("工具执行错误: {}", e);
                        tracing::warn!("[Agent] 工具 {} 执行失败: {}", tc_name, e);

                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_error".to_string(),
                                    content: err_msg.clone(),
                                    tool_name: Some(tc_name.clone()),
                                    tool_result: None,
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }

                        self.update_task_progress(&tc_name, &err_msg, false, None);
                        self.send_task_progress(&step_tx).await;

                        err_msg
                    }
                };

                self.session.push_tool_result(&tc_id, &tc_name, &tool_result);
            }
        }

        tracing::info!(
            "[Agent] ReAct 完成: {} 次迭代, {} 字符输出, max_iterations_reached={}",
            iterations,
            final_content.len(),
            max_iterations_reached
        );

        let first_reasoning = self.session.messages.iter()
            .find(|m| m.role == "assistant" && m.first_reasoning.is_some())
            .and_then(|m| m.first_reasoning.clone());

        let cancelled = cancel.map_or(false, |c| c.load(std::sync::atomic::Ordering::Relaxed));
        
        let task_progress = self.task_plan.as_ref().map(|p| TaskProgress::from(p));

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
            reasoning: None,
            cancelled,
            max_iterations_reached,
            task_plan: self.task_plan.clone(),
            task_progress,
            detection_result: self.detection_result.clone(),
        })
    }

    async fn try_parse_task_plan(&mut self, content: &str, step_tx: &Option<mpsc::Sender<AgentStep>>) {
        if let Some(result) = TaskDecompositionParser::parse(content) {
            self.task_plan = Some(result.plan.clone());
            
            let issues = TaskDecompositionParser::validate_plan(&result.plan);
            if !issues.is_empty() {
                tracing::warn!("[Agent] 任务计划验证发现问题: {:?}", issues);
            }

            tracing::info!("[Agent] 解析到任务计划: {} 个子任务", result.plan.tasks.len());
            
            if let Some(ref tx) = step_tx {
                let _ = tx
                    .send(AgentStep {
                        step_type: "task_plan".to_string(),
                        content: serde_json::to_string(&result.plan).unwrap_or_default(),
                        tool_name: None,
                        tool_result: None,
                        turn: 0,
                        max_turns: self.max_iterations,
                    })
                    .await;
            }
        }
    }

    fn filter_duplicate_tool_calls(&self, tool_calls: &[AgentToolCall]) -> Vec<AgentToolCall> {
        tool_calls
            .iter()
            .filter(|tc| {
                let key = format!("{}_{}", tc.name, tc.arguments);
                !self.executed_tools.contains(&key)
            })
            .cloned()
            .collect()
    }

    fn update_task_progress(&mut self, tool_name: &str, result: &str, success: bool, quality_score: Option<f64>) {
        if let Some(ref mut plan) = self.task_plan {
            for task in plan.tasks.iter_mut() {
                if task.tool_name.as_deref() == Some(tool_name) && task.status == TaskStatus::Pending {
                    if success {
                        task.mark_completed(result, quality_score);
                        self.completed_tasks.insert(task.id.clone());
                    } else {
                        task.mark_failed(result);
                    }
                    break;
                }
            }
        }
    }

    async fn send_task_progress(&self, step_tx: &Option<mpsc::Sender<AgentStep>>) {
        if let (Some(ref tx), Some(ref plan)) = (step_tx, &self.task_plan) {
            let progress = TaskProgress::from(plan);
            let _ = tx
                .send(AgentStep {
                    step_type: "task_progress".to_string(),
                    content: serde_json::to_string(&progress).unwrap_or_default(),
                    tool_name: None,
                    tool_result: None,
                    turn: 0,
                    max_turns: self.max_iterations,
                })
                .await;
        }
    }

    async fn call_llm_with_tools_and_retry(
        &mut self,
        system_prompt: &str,
        step_tx: &Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<(AgentMessage, Vec<String>, bool), AppError> {
        let mut attempts = 0u32;
        loop {
            match self.call_llm_with_tools(system_prompt, step_tx, cancel.clone()).await {
                Ok((msg, blocks, cancelled, input_tokens, output_tokens)) => {
                    if input_tokens > 0 || output_tokens > 0 {
                        self.session.total_input_tokens += input_tokens;
                        self.session.total_output_tokens += output_tokens;
                        tracing::info!(
                            "[Agent] Token 写回完成 — 本次输入: {}, 输出: {}, 累计输入: {}, 累计输出: {}",
                            input_tokens,
                            output_tokens,
                            self.session.total_input_tokens,
                            self.session.total_output_tokens
                        );
                    }
                    return Ok((msg, blocks, cancelled));
                }
                Err(e) if attempts < self.max_retries => {
                    attempts += 1;
                    let wait_secs = 2u64.pow(attempts);
                    tracing::warn!(
                        "[Agent] LLM 请求失败（第 {}/{} 次重试，{}s 后重试）: {}",
                        attempts,
                        self.max_retries,
                        wait_secs,
                        e
                    );
                    if let Some(ref tx) = step_tx {
                        let _ = tx.send(AgentStep {
                            step_type: "retry".to_string(),
                            content: format!(
                                "LLM 请求失败，{}s 后重试（{}/{}）: {}",
                                wait_secs, attempts, self.max_retries, e
                            ),
                            tool_name: None,
                            tool_result: None,
                            turn: 0,
                            max_turns: self.max_iterations,
                        }).await;
                    }
                    tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                }
                Err(e) => {
                    tracing::error!(
                        "[Agent] LLM 请求在 {} 次重试后仍然失败: {}",
                        self.max_retries,
                        e
                    );
                    return Err(e);
                }
            }
        }
    }

    async fn call_llm_with_tools(
        &self,
        system_prompt: &str,
        step_tx: &Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<(AgentMessage, Vec<String>, bool, u64, u64), AppError> {
        let tools = self.tool_registry.get_schemas().await;
        let tool_count = tools.len();

        let mut messages: Vec<ChatMessage> = vec![ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }];

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
                reasoning_content: None,
            });
        }

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

        tracing::info!(
            "[Agent] 发送 LLM 请求（{} 工具，{} 历史消息）",
            tool_count,
            self.session.messages.len()
        );

        let cancel_flag = cancel.clone();
        let mut event_rx = self.llm_client.chat_stream(&request, cancel.clone()).await?;

        let mut full_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut was_cancelled = false;
        let mut sent_tool_call_indices: HashSet<usize> = HashSet::new();
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;

        while let Some(event) = event_rx.recv().await {
            if let Some(ref flag) = cancel_flag {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    was_cancelled = true;
                    break;
                }
            }
            match event {
                StreamEvent::TextDelta(text) => {
                    full_content.push_str(&text);
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

                    if !name.is_empty() && !sent_tool_call_indices.contains(&index) {
                        sent_tool_call_indices.insert(index);
                        
                        // 格式化工具调用显示信息
                        let display_content = format_tool_call_display(&name, &accumulated_tool_calls[index].arguments);
                        
                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_call".to_string(),
                                    content: display_content,
                                    tool_name: Some(name.clone()),
                                    tool_result: None,
                                    turn: 0,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }
                    }
                }
                StreamEvent::Usage { prompt_tokens, completion_tokens } => {
                    input_tokens = prompt_tokens;
                    output_tokens = completion_tokens;
                    tracing::debug!(
                        "[Agent] Token 用量 — 输入: {}, 输出: {}",
                        input_tokens,
                        output_tokens
                    );
                }
                StreamEvent::Done(_) => {
                    break;
                }
                StreamEvent::Error(err) => {
                    return Err(AppError::LlmError(err));
                }
            }
        }

        if input_tokens > 0 || output_tokens > 0 {
            tracing::info!(
                "[Agent] 本次请求 Token 用量 — 输入: {}, 输出: {}, 累计输入: {}, 累计输出: {}",
                input_tokens,
                output_tokens,
                self.session.total_input_tokens + input_tokens,
                self.session.total_output_tokens + output_tokens
            );
        }

        let reasoning_blocks = CotExtractor::extract_multiple(
            &full_content,
            if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(&accumulated_reasoning)
            },
        );

        let cleaned_content = if reasoning_blocks.is_empty() {
            full_content.clone()
        } else {
            let re = regex::Regex::new(r"(?is)<think\s*>[\s\S]*?</think\s*>").unwrap();
            re.replace_all(&full_content, "").trim().to_string()
        };

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

        Ok((agent_msg, reasoning_blocks, was_cancelled, input_tokens, output_tokens))
    }

    fn build_system_prompt(&self) -> String {
        if let Some(ref override_prompt) = self.session.system_prompt_override {
            return override_prompt.clone();
        }

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

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }

    pub fn task_plan(&self) -> Option<&TaskPlan> {
        self.task_plan.as_ref()
    }
}