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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

// 从 AppConfig 读取，保留默认值作为兜底
pub const COMPACT_KEEP_LAST_FALLBACK: usize = 20;

/// 格式化工具调用显示信息
/// 将 JSON 参数转换为易读的格式，特别是文件类工具显示相对路径和文件名
#[allow(dead_code)]
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
    pub again_reasonings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub cancelled: bool,
    pub max_iterations_reached: bool,
}

pub struct AgentRuntime {
    session: AgentSession,
    llm_client: LlmClient,
    tool_registry: Arc<ToolRegistry>,
    config: AppConfig,
    max_iterations: usize,
    max_retries: u32,
    has_first_reasoning: bool,
    accumulated_again_reasonings: Vec<String>,
    skills: Vec<SkillDef>,
    executed_tools: HashSet<String>,
    /// 同一工具+参数的重试次数，超过限制后强制跳过
    tool_retry_count: HashMap<String, u32>,
    /// doom-loop 检测：连续相同工具调用的次数
    consecutive_doom_count: u32,
    /// doom-loop 检测：上一次工具调用的去重 key
    last_doom_key: Option<String>,
    /// 是否已进入优雅终止（最后一次无工具调用）
    grace_terminating: bool,
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
            accumulated_again_reasonings: Vec::new(),
            skills,
            executed_tools: HashSet::new(),
            tool_retry_count: HashMap::new(),
            consecutive_doom_count: 0,
            last_doom_key: None,
            grace_terminating: false,
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

        self.session.push_user(user_input);

        let compact_keep = if self.config.compact_keep > 0 {
            self.config.compact_keep
        } else {
            COMPACT_KEEP_LAST_FALLBACK
        };
        if self.config.compact_threshold > 0 && self.session.message_count() > self.config.compact_threshold {
            tracing::info!(
                "[Agent] 消息数 {} 超过阈值 {}，触发上下文压缩，保留最近 {} 条",
                self.session.message_count(),
                self.config.compact_threshold,
                compact_keep
            );
            self.session.compact(compact_keep);
            tracing::info!(
                "[Agent] 压缩完成，当前消息数: {}，累计压缩次数: {}",
                self.session.message_count(),
                self.session.compaction_count
            );
        }

        let system_prompt = self.build_system_prompt();

        loop {
            iterations += 1;

            // max_iterations == 0 表示无限制
            if self.max_iterations > 0 && iterations > self.max_iterations {
                if self.grace_terminating {
                    // 优雅终止已完成，退出循环
                    tracing::warn!(
                        "[Agent] 达到最大迭代次数 {}，优雅终止完成",
                        self.max_iterations
                    );
                    max_iterations_reached = true;
                    break;
                }
                // 第一次达到上限：注入总结提示词，剥离工具，做最后一次无工具调用
                tracing::warn!(
                    "[Agent] 达到最大迭代次数 {}，进入优雅终止（最后一次无工具调用）",
                    self.max_iterations
                );
                self.grace_terminating = true;
                max_iterations_reached = true;

                // 注入 user 消息要求 LLM 总结
                let summary_prompt = format!(
                    "[Agent 已达最大迭代次数 {}，请提供当前工作的完整总结，包括已完成的内容和任何未完成的事项]",
                    self.max_iterations
                );
                self.session.push_user(&summary_prompt);

                // 用无工具的调用做最后一次 LLM 响应
                let (summary_msg, _, cancelled) = self
                    .call_llm_with_tools_and_retry(&system_prompt, &step_tx, cancel.clone())
                    .await?;

                if cancelled {
                    final_content = summary_msg.content.clone();
                    break;
                }

                final_content = summary_msg.content.clone();
                self.session.push_message(summary_msg);
                continue;
            }

            tracing::info!("[Agent] ReAct 迭代 {}/{}", iterations, self.max_iterations);

            let (assistant_message, reasoning_blocks, cancelled) = self
                .call_llm_with_tools_and_retry(&system_prompt, &step_tx, cancel.clone())
                .await?;

            if cancelled {
                final_content = assistant_message.content.clone();
                break;
            }

            let msg_for_session = assistant_message.clone();

            // first_thought/thought 步骤已在 call_llm_with_tools 中按正确顺序发送
            // 此处仅累积推理内容用于最终结果返回
            if !reasoning_blocks.is_empty() {
                self.accumulated_again_reasonings.extend(reasoning_blocks.clone());
            }

            let tool_calls: Vec<AgentToolCall> = assistant_message
                .tool_calls
                .clone()
                .unwrap_or_default();

            // 先过滤重复工具调用，再推入会话，避免 assistant 消息带有 tool_calls
            // 但后续缺少对应的 tool 响应（违反 OpenAI API 协议）
            let valid_tool_calls = self.filter_duplicate_tool_calls(&tool_calls);
            let has_filtered = valid_tool_calls.len() < tool_calls.len();

            if has_filtered {
                // 创建只含有效 tool_calls 的 assistant 消息推入会话
                let mut clean_msg = assistant_message.clone();
                clean_msg.tool_calls = if valid_tool_calls.is_empty() {
                    None
                } else {
                    Some(valid_tool_calls.clone())
                };
                self.session.push_message(clean_msg);
            } else {
                self.session.push_message(msg_for_session);
            }

            if tool_calls.is_empty() {
                final_content = assistant_message.content.clone();
                break;
            }

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
                let args_json = tc.arguments.clone();
                let ws = self.session.workspace.clone();
                let mut args: serde_json::Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::Value::Null);
                let session_id = self.session.id.clone();
                // 注入会话 ID，供 cron 等工具使用
                if let Some(obj) = args.as_object_mut() {
                    obj.insert("_session_id".to_string(), serde_json::json!(session_id));
                }
                let step_tx = step_tx.clone();
                let iterations = iterations;
                let max_iterations = self.max_iterations;
                let name_clone_for_spawn = name.clone();
                async move {
                    // 为 execute_command/terminal 工具创建流式输出通道
                    let chunk_tx: Option<mpsc::UnboundedSender<String>> = if name == "execute_command" || name == "terminal" {
                        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                        let fwd_tx = step_tx.clone();
                        let spawn_name = name_clone_for_spawn.clone();
                        tokio::spawn(async move {
                            while let Some(chunk) = rx.recv().await {
                                if let Some(ref tx) = fwd_tx {
                                    let _ = tx
                                        .send(AgentStep {
                                            step_type: "tool_chunk".to_string(),
                                            content: chunk,
                                            tool_name: Some(spawn_name.clone()),
                                            tool_result: None,
                                            turn: iterations,
                                            max_turns: max_iterations,
                                        })
                                        .await;
                                }
                            }
                        });
                        Some(tx)
                    } else {
                        None
                    };

                    let result = registry.execute(&name, args, ws.as_deref(), chunk_tx).await;
                    (id, name, args_json, result)
                }
            }).collect();

            let tool_results = futures::future::join_all(tool_futures).await;

            for (tc_id, tc_name, tc_args_json, result) in tool_results {
                // 基于 name+参数内容的去重 key，相同参数视为重复调用
                let key = Self::tool_call_dedup_key(&tc_name, &tc_args_json);
                self.executed_tools.insert(key.clone());

                let tool_result = match result {
                    Ok(output) => {
                        let truncated = if output.len() > 8000 {
                            // 安全截断，避免 UTF-8 字符边界溢出
                            let mut end = 8000;
                            while !output.is_char_boundary(end) {
                                end -= 1;
                            }
                            format!(
                                "{}...\n\n[结果已截断，原长度: {} 字符]",
                                &output[..end],
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
                                    tool_result: Some({
                                        let max_len = truncated.len().min(500);
                                        let mut end = max_len;
                                        while !truncated.is_char_boundary(end) {
                                            end -= 1;
                                        }
                                        truncated[..end].to_string()
                                    }),
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                })
                                .await;
                        }

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

                        err_msg
                    }
                };

                // 明确标注工具返回的是真实数据，避免小模型误判为帮助信息
                let contextualized = format!("← {} 工具返回的数据（实时读取结果，非帮助信息）:\n{}", tc_name, tool_result);
                self.session.push_tool_result(&tc_id, &tc_name, &contextualized);

                // 累加重试计数（同 key 递增，用于跨迭代硬限制）
                *self.tool_retry_count.entry(key.clone()).or_insert(0) += 1;
                if *self.tool_retry_count.get(&key).unwrap_or(&0) >= 2 {
                    tracing::warn!("[Agent] 工具 {} 同一参数已执行超过2次，后续调用将被强制跳过", tc_name);
                }
            }

            // doom-loop 检测：连续同一工具+参数调用超过 3 次时熔断
            if !valid_tool_calls.is_empty() {
                let first_key = Self::tool_call_dedup_key(&valid_tool_calls[0].name, &valid_tool_calls[0].arguments);
                if let Some(ref last) = self.last_doom_key {
                    if last == &first_key {
                        self.consecutive_doom_count += 1;
                    } else {
                        self.consecutive_doom_count = 1;
                        self.last_doom_key = Some(first_key);
                    }
                } else {
                    self.consecutive_doom_count = 1;
                    self.last_doom_key = Some(first_key);
                }

                if self.consecutive_doom_count >= 3 {
                    // 对批次中所有工具强制标记为已执行，避免下次继续
                    for tc in &valid_tool_calls {
                        let k = Self::tool_call_dedup_key(&tc.name, &tc.arguments);
                        self.executed_tools.insert(k.clone());
                    }
                    tracing::warn!(
                        "[Agent] doom-loop 检测: 连续 {} 次相同工具调用 '{}'，强制熔断",
                        self.consecutive_doom_count,
                        valid_tool_calls[0].name
                    );
                }
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

        Ok(AgentResult {
            session_id: self.session.id.clone(),
            content: final_content,
            iterations,
            messages: self.session.messages.clone(),
            first_reasoning,
            again_reasonings: if self.accumulated_again_reasonings.is_empty() {
                None
            } else {
                Some(self.accumulated_again_reasonings.clone())
            },
            reasoning: None,
            cancelled,
            max_iterations_reached,
        })
    }

    /// 生成工具调用的去重 key（基于 name + 参数内容）
    fn tool_call_dedup_key(name: &str, args: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hasher);
        args.hash(&mut hasher);
        format!("t{}", hasher.finish())
    }

    fn filter_duplicate_tool_calls(&self, tool_calls: &[AgentToolCall]) -> Vec<AgentToolCall> {
        let mut seen_in_batch = std::collections::HashSet::new();
        let mut result = Vec::new();
        for tc in tool_calls {
            let key = Self::tool_call_dedup_key(&tc.name, &tc.arguments);
            // 跳过已执行过的（跨迭代去重）
            if self.executed_tools.contains(&key) {
                continue;
            }
            // 跳过本次批次中已出现过的（同批次去重）
            if !seen_in_batch.insert(key.clone()) {
                continue;
            }
            // 同一工具+参数已重试超过2次，强制跳过
            if let Some(count) = self.tool_retry_count.get(&key) {
                if *count >= 2 {
                    tracing::warn!("[Agent] 工具 {} 已重试 {} 次，强制跳过", tc.name, count);
                    continue;
                }
            }
            result.push(tc.clone());
        }
        result
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
        &mut self,
        system_prompt: &str,
        step_tx: &Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<(AgentMessage, Vec<String>, bool, u64, u64), AppError> {
        let tools = if self.grace_terminating {
            // 优雅终止：不传工具，LLM 只能返回文本
            Vec::new()
        } else {
            self.tool_registry.get_schemas().await
        };
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
                reasoning_content: msg.reasoning.clone(),
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
            stream_options: Some(serde_json::json!({"include_usage": true})),
        };

        tracing::info!(
            "[Agent] 发送 LLM 请求（{} 工具，{} 历史消息）",
            tool_count,
            self.session.messages.len()
        );

        let cancel_flag = cancel.clone();
        let mut stream_handle = self.llm_client.chat_stream(&request, cancel.clone()).await?;

        let mut full_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut was_cancelled = false;
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;

        while let Some(event) = stream_handle.rx.recv().await {
            if let Some(ref flag) = cancel_flag {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    was_cancelled = true;
                    // 立即终止底层的 HTTP 流任务，强制关闭与 LLM 服务的连接
                    stream_handle.abort();
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
                    accumulated_tool_calls[index].arguments = arguments.clone();
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

        // 在流结束后、返回之前，按正确顺序发送 first_thought → tool_call
        // 确保前端按 思考→工具调用 的正确顺序渲染
        if let Some(ref tx) = step_tx {
            // 1. 先发送推理完成事件
            if !reasoning_blocks.is_empty() {
                for (idx, block) in reasoning_blocks.iter().enumerate() {
                    let step_type = if idx == 0 && !self.has_first_reasoning {
                        "first_thought"
                    } else {
                        "thought"
                    };
                    let _ = tx
                        .send(AgentStep {
                            step_type: step_type.to_string(),
                            content: block.clone(),
                            tool_name: None,
                            tool_result: None,
                            turn: 0,
                            max_turns: self.max_iterations,
                        })
                        .await;
                }
            }

            // 2. 再发送所有累积的工具调用事件
            for tc in &accumulated_tool_calls {
                if !tc.name.is_empty() {
                    let _ = tx
                        .send(AgentStep {
                            step_type: "tool_call".to_string(),
                            content: tc.arguments.clone(),
                            tool_name: Some(tc.name.clone()),
                            tool_result: None,
                            turn: 0,
                            max_turns: self.max_iterations,
                        })
                        .await;
                }
            }
        }

        let tool_calls = if accumulated_tool_calls.is_empty() {
            None
        } else {
            Some(accumulated_tool_calls)
        };

        let is_first_llm_call = !self.has_first_reasoning;

        let (first_reasoning, again_reasonings) = if reasoning_blocks.is_empty() {
            (None, None)
        } else if is_first_llm_call {
            // 首次思考：第一个推理块作为 first_reasoning，其余作为 again_reasonings
            let first = reasoning_blocks.first().cloned();
            let rest = if reasoning_blocks.len() > 1 {
                Some(reasoning_blocks[1..].to_vec())
            } else {
                None
            };
            (first, rest)
        } else {
            // 非首次思考：所有推理块都作为 again_reasonings
            (None, Some(reasoning_blocks.clone()))
        };

        // 标记首次 LLM 调用完成（在 first_reasoning/again_reasonings 计算之后）
        self.has_first_reasoning = true;

        let reasoning = if accumulated_reasoning.is_empty() {
            None
        } else {
            Some(accumulated_reasoning)
        };

        let agent_msg = AgentMessage {
            role: "assistant".to_string(),
            content: cleaned_content,
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            first_reasoning,
            again_reasonings,
            reasoning,
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
}