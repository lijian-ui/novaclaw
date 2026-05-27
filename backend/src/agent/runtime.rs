use crate::agent::cot::CotExtractor;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::config::AppConfig;
use crate::llm::client::LlmClient;
use crate::llm::types::{ChatMessage, ChatRequest, StreamEvent};
use crate::llm::deepseek_template;
use crate::llm::tokenizer; // 新增
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
        format!("{}: {}...", tool_name, crate::utils::safe_truncate(&arguments, 200))
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
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cached_tokens: u64,
    pub last_input_tokens: u64,
    pub last_output_tokens: u64,
    pub cache_hit_rate: f64,
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
    /// 累计缓存 Token
    total_cached_tokens: u64,
    /// 最后一次 LLM 请求的输入 Token（"本次输入"）
    last_input_tokens: u64,
    /// 最后一次 LLM 请求的输出 Token（"本次输出"）
    last_output_tokens: u64,
    /// 易变后缀（memory + 日期 + 环境 + 技能），每次 run_turn 构建
    volatile_suffix: Option<String>,

    // ── 成本控制（DeepSeek 特化） ──
    /// 下一轮是否强制升级到 Pro 模型（由 /pro 命令触发）
    next_turn_pro: bool,
    /// 缓存的 Pro 模型名称（查找到后缓存，避免每次查找）
    cached_pro_model: Option<String>,
    /// 当前轮是否已升级到 Pro
    current_turn_pro: bool,
    /// 连续工具调用失败次数（用于失败触发升级）
    consecutive_tool_failures: u32,
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
            total_cached_tokens: 0,
            last_input_tokens: 0,
            last_output_tokens: 0,
            volatile_suffix: None,
            next_turn_pro: false,
            cached_pro_model: Self::find_pro_model_static(),
            current_turn_pro: false,
            consecutive_tool_failures: 0,
        }
    }

    // ── 工具结果压缩阈值（超过此字符数的工具结果，在轮次结束后压缩） ──
    const TOOL_RESULT_COMPRESS_LIMIT: usize = 6000;

    // ── 预检：消息历史总字符数触发压缩的阈值 ──
    const PREFLIGHT_CHAR_LIMIT: usize = 500_000;
    // ── 预检：序列化请求体字节硬限制（超过此值强制截断） ──
    const PREFLIGHT_BODY_BYTE_HARD_LIMIT: usize = 3_000_000; // ~3MB，DeepSeek 1M context 的安全边界
    // ── 预检层级阈值（相对于模型上下文窗口的比例） ──
    const PREFLIGHT_LEVEL1_RATIO: f64 = 0.70; // 告警
    const PREFLIGHT_LEVEL2_RATIO: f64 = 0.85; // 压缩
    const PREFLIGHT_LEVEL3_RATIO: f64 = 0.95; // 强制截断

    pub async fn run_turn(
        &mut self,
        user_input: &str,
        step_tx: Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        images: &[String],
    ) -> Result<AgentResult, AppError> {
        let mut iterations = 0;
        let mut final_content = String::new();
        let mut max_iterations_reached = false;

        // ── P0: 成本控制 - 模型升级决议 ──
        self.current_turn_pro = false;
        if self.next_turn_pro {
            // /pro 命令强制升级
            self.current_turn_pro = true;
            self.next_turn_pro = false;
            tracing::info!("[Cost] ⬆️ 用户 `/pro` 触发，本轮升级到 Pro 模型");
        }
        if let Some(ref pro_model) = self.cached_pro_model.clone() {
            if !self.current_turn_pro && self.consecutive_tool_failures >= 3 {
                // 连续 3+ 次工具调用失败自动升级
                self.current_turn_pro = true;
                tracing::info!(
                    "[Cost] ⬆️ 连续 {} 次工具调用失败，自动升级到 Pro 模型 ({})",
                    self.consecutive_tool_failures,
                    pro_model
                );
            }
        }
        // 解析本轮实际使用的模型
        let resolved_model = if self.current_turn_pro {
            self.cached_pro_model.clone().unwrap_or_else(|| self.session.model.clone())
        } else {
            self.session.model.clone()
        };
        // 临时替换 session.model 用于本轮
        let original_model = self.session.model.clone();
        if resolved_model != original_model {
            self.session.model = resolved_model.clone();
        }

        // ── 处理特殊命令 ──
        let processed_input = if user_input.trim().starts_with("/pro") {
            self.next_turn_pro = true;
            tracing::info!("[Cost] 🚀 检测到 /pro 命令，下一轮将升级到 Pro 模型");
            let trimmed = user_input.trim_start_matches("/pro").trim();
            if trimmed.is_empty() {
                "请继续，使用更强大的模型来处理此请求".to_string()
            } else {
                trimmed.to_string()
            }
        } else {
            user_input.to_string()
        };

        self.session.push_user_with_images(&processed_input, images);

        // ── P0: 上下文压缩检查 ──
        let compact_keep = if self.config.compact_keep > 0 {
            self.config.compact_keep
        } else {
            COMPACT_KEEP_LAST_FALLBACK
        };
        if self.config.compact_threshold > 0 && self.session.message_count() > self.config.compact_threshold {
            let keep = compact_keep;
            tracing::info!(
                "[Agent] 消息数 {} 超过阈值 {}，触发上下文压缩 (compact_in_place)，保留最近 {} 条",
                self.session.message_count(),
                self.config.compact_threshold,
                keep
            );

            // P1: 生成 AI 摘要（优先使用 flash 模型，降级到当前模型）
            let ai_summary = self.generate_ai_summary(keep).await;
            self.session.compact_in_place(keep, ai_summary);

            tracing::info!(
                "[Agent] 压缩完成，当前消息数: {}，累计压缩次数: {}",
                self.session.message_count(),
                self.session.compaction_count
            );
        }

        // ── P0: 构建并冻结 system prompt（带指纹检测） ──
        let frozen = self.build_frozen_system_prompt().await;
        match self.session.set_frozen_system_prompt(frozen) {
            Ok(is_first) => {
                if is_first {
                    tracing::info!("[Cache] frozen_system_prompt 首次设置完成");
                } else {
                    tracing::debug!("[Cache] frozen_system_prompt 无变化，缓存前缀稳定");
                }
            }
            Err(msg) => {
                tracing::warn!("[Cache] {} 本次请求缓存前缀已更新，将触发 cache miss", msg);
                // 指纹变化意味着前缀更新，reset 失效率
                self.session.prefix_invalidated = true;
            }
        }
        // volatile 后缀每次构建（含 memory、日期等变化信息）
        self.volatile_suffix = Some(self.build_volatile_suffix().await);

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

                // 注入 user 消息要求 LLM 生成结构化摘要
                let summary_prompt = format!(
                    "[对话已达 {} 次迭代上限。请生成以下格式的结构化摘要：]\n\n\
                    ## Goal\n- [单句目标描述]\n\n\
                    ## Progress\n### Done\n- [已完成工作]\n### In Progress\n- [进行中]\n### Blocked\n- [阻塞项]\n\n\
                    ## Decisions Made\n- [关键决策及理由]\n\n\
                    ## Critical Context\n- [重要技术细节、错误、开放问题]\n\n\
                    ## Next Steps\n- [有序的下一步行动]\n\n\
                    此外，如果对话中出现了需要跨会话记住的持久事实（用户偏好、项目约定），请用 memory 工具的 add action 保存。",
                    self.max_iterations
                );
                self.session.push_user(&summary_prompt);

                // ⚠️ 预检：优雅终止前检查上下文，避免超长请求浪费
                self.maybe_compact_for_preflight().await;

                // 用无工具的调用做最后一次 LLM 响应
                let (summary_msg, _, cancelled, _) = self
                    .call_llm_with_tools_and_retry(&step_tx, cancel.clone())
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

            // ⚠️ 预检：每轮 LLM 调用前检查上下文大小
            self.maybe_compact_for_preflight().await;

            let (assistant_message, reasoning_blocks, cancelled, _) = self
                .call_llm_with_tools_and_retry(&step_tx, cancel.clone())
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

                // ── 成本控制：检测自动升级标记 ──
                if final_content.contains("<<<NEEDS_PRO>>>") {
                    if let Some(ref pro_model) = self.cached_pro_model.clone() {
                        self.current_turn_pro = true;
                        self.session.model = pro_model.clone();
                        tracing::info!("[Cost] ⬆️ 检测到 <<<NEEDS_PRO>>> 标记，升级到 Pro 模型 ({})", pro_model);
                        // 清理标记并重试本轮
                        final_content = final_content.replace("<<<NEEDS_PRO>>>", "").trim().to_string();
                    }
                }

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
                            approval: None,
                            approval_id: None,
                            cached_tokens: None,
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
                    // 为 execute_command/terminal/delegate_task 工具创建流式输出通道
                    let chunk_tx: Option<mpsc::UnboundedSender<String>> = if name == "execute_command" || name == "terminal" || name == "delegate_task" {
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
                                                    approval: None,
                                                    approval_id: None,
                                                    cached_tokens: None,
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

                match result {
                    Ok(crate::tools::types::ToolResult::Success(output)) => {
                        // 工具执行成功：重置失败计数
                        self.consecutive_tool_failures = 0;
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
                                    approval: None,
                                    approval_id: None,
                                    cached_tokens: None,
                                })
                                .await;
                        }

                        // 明确标注工具返回的是真实数据，避免小模型误判为帮助信息
                        let contextualized = format!("← {} 工具返回的数据（实时读取结果，非帮助信息）:\n{}", tc_name, truncated);
                        self.session.push_tool_result(&tc_id, &tc_name, &contextualized);

                        // 累加重试计数（同 key 递增，用于跨迭代硬限制）
                        *self.tool_retry_count.entry(key.clone()).or_insert(0) += 1;
                        if *self.tool_retry_count.get(&key).unwrap_or(&0) >= 2 {
                            tracing::warn!("[Agent] 工具 {} 同一参数已执行超过2次，后续调用将被强制跳过", tc_name);
                        }
                    }
                    Ok(crate::tools::types::ToolResult::PendingApproval(approval)) => {
                        // 生成确认 ID
                        let approval_id = format!("approval_{}", uuid::Uuid::new_v4().to_string());
                        
                        tracing::info!("[Agent] 工具 {} 需要用户确认，ID: {}", tc_name, approval_id);

                        // 保存到全局状态
                        {
                            let state = crate::APP_STATE.read().await;
                            state.approval_manager.add_pending(
                                approval_id.clone(),
                                approval.clone(),
                                self.session.id.clone(),
                                tc_name.clone(),
                                tc_args_json.clone(),
                            ).await;
                        }

                        // 发送确认事件到前端
                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "approval_required".to_string(),
                                    content: approval.message.clone(),
                                    tool_name: Some(tc_name.clone()),
                                    tool_result: None,
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                    approval: Some(approval),
                                    approval_id: Some(approval_id.clone()),
                                    cached_tokens: None,
                                })
                                .await;
                        }

                        // 告诉 LLM 正在等待用户确认
                        let wait_msg = format!("Waiting for user approval to proceed with {} operation.", tc_name);
                        let contextualized = format!("← {} 工具等待用户确认:\n{}", tc_name, wait_msg);
                        self.session.push_tool_result(&tc_id, &tc_name, &contextualized);
                    }
                    Err(e) => {
                        let err_msg = format!("工具执行错误: {}", e);
                        self.consecutive_tool_failures += 1;
                        tracing::warn!("[Agent] 工具 {} 执行失败: {} (连续失败: {})", tc_name, e, self.consecutive_tool_failures);

                        // 检查是否需要升级
                        if self.consecutive_tool_failures >= 3 {
                            if let Some(ref pro_model) = self.cached_pro_model.clone() {
                                if !self.current_turn_pro {
                                    self.current_turn_pro = true;
                                    self.session.model = pro_model.clone();
                                    tracing::info!("[Cost] ⬆️ 连续 {} 次工具失败，本轮升级到 Pro 模型 ({})", self.consecutive_tool_failures, pro_model);
                                }
                            }
                        }

                        if let Some(ref tx) = step_tx {
                            let _ = tx
                                .send(AgentStep {
                                    step_type: "tool_error".to_string(),
                                    content: err_msg.clone(),
                                    tool_name: Some(tc_name.clone()),
                                    tool_result: None,
                                    turn: iterations,
                                    max_turns: self.max_iterations,
                                    approval: None,
                                    approval_id: None,
                                    cached_tokens: None,
                                })
                                .await;
                        }

                        // 明确标注工具返回的是真实数据，避免小模型误判为帮助信息
                        let contextualized = format!("← {} 工具返回的数据（实时读取结果，非帮助信息）:\n{}", tc_name, err_msg);
                        self.session.push_tool_result(&tc_id, &tc_name, &contextualized);

                        // 累加重试计数（同 key 递增，用于跨迭代硬限制）
                        *self.tool_retry_count.entry(key.clone()).or_insert(0) += 1;
                        if *self.tool_retry_count.get(&key).unwrap_or(&0) >= 2 {
                            tracing::warn!("[Agent] 工具 {} 同一参数已执行超过2次，后续调用将被强制跳过", tc_name);
                        }
                    }
                };
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

        // ── P1: 轮末压缩大工具结果 — 超过阈值的 tool 结果压缩为摘要 ──
        // 当前轮模型看到完整结果，下一轮起看到压缩版
        let mut compressed_count = 0u32;
        for msg in self.session.messages.iter_mut().rev() {
            if msg.role == "tool" && msg.content.len() > Self::TOOL_RESULT_COMPRESS_LIMIT {
                let full_len = msg.content.len();
                msg.content = format!(
                    "[工具结果已压缩: 原始 {} 字符 | {} 行]\n\n{}",
                    full_len,
                    full_len / 80, // 粗略行数
                    &msg.content[..Self::TOOL_RESULT_COMPRESS_LIMIT.min(full_len)]
                );
                compressed_count += 1;
            }
        }
        if compressed_count > 0 {
            tracing::info!("[Compress] 轮末压缩了 {} 个大工具结果", compressed_count);
        }

        tracing::info!(
            "[Agent] ReAct 完成: {} 次迭代, {} 字符输出, max_iterations_reached={}. Token: 本次输入 {}, 本次输出 {}, 缓存 {}, 累计输入 {}, 累计输出 {}, 缓存命中率 {:.1}%",
            iterations,
            final_content.len(),
            max_iterations_reached,
            self.last_input_tokens,
            self.last_output_tokens,
            self.total_cached_tokens,
            self.session.total_input_tokens,
            self.session.total_output_tokens,
            self.session.cache_hit_rate() * 100.0
        );

        // ── 成本控制：模型恢复 ──
        if original_model != self.session.model {
            let used_model = self.session.model.clone();
            self.session.model = original_model;
            tracing::info!("[Cost] 本轮使用 {}, 已恢复默认模型 {}", used_model, self.session.model);
        }
        if self.current_turn_pro {
            tracing::info!("[Cost] 本轮成本: Pro 模型 (较高)");
        } else {
            tracing::debug!("[Cost] 本轮成本: 默认模型 (标准)");
        }

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
            total_input_tokens: self.session.total_input_tokens,
            total_output_tokens: self.session.total_output_tokens,
            total_cached_tokens: self.total_cached_tokens,
            last_input_tokens: self.last_input_tokens,
            last_output_tokens: self.last_output_tokens,
            cache_hit_rate: self.session.cache_hit_rate(),
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
        step_tx: &Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<(AgentMessage, Vec<String>, bool, u64), AppError> {
        let mut attempts = 0u32;
        loop {
            match self.call_llm_with_tools(step_tx, cancel.clone()).await {
                Ok((msg, blocks, cancelled, input_tokens, output_tokens, cached_tokens)) => {
                    if input_tokens > 0 || output_tokens > 0 {
                        self.session.total_input_tokens += input_tokens;
                        self.session.total_output_tokens += output_tokens;
                        self.total_cached_tokens += cached_tokens;
                        self.last_input_tokens = input_tokens;
                        self.last_output_tokens = output_tokens;
                        // 统计缓存命中/未命中
                        if cached_tokens > 0 {
                            self.session.cache_hit_tokens += cached_tokens;
                            self.session.cache_miss_tokens += input_tokens.saturating_sub(cached_tokens);
                        } else {
                            self.session.cache_miss_tokens += input_tokens;
                        }
                    }
                    return Ok((msg, blocks, cancelled, cached_tokens));
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
                            approval: None,
                            approval_id: None,
                            cached_tokens: None,
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
        step_tx: &Option<mpsc::Sender<AgentStep>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<(AgentMessage, Vec<String>, bool, u64, u64, u64), AppError> {
        // 使用会话级冻结的 system prompt（确保每轮字节序列一致）
        let system_prompt = self.session.frozen_system_prompt
            .as_ref()
            .expect("[Cache] frozen_system_prompt 必须在首次调用前设置")
            .clone();

        let tools = if self.grace_terminating {
            // 优雅终止：不传工具，LLM 只能返回文本
            Vec::new()
        } else {
            self.tool_registry.get_schemas().await
        };
        let tool_count = tools.len();

        let mut messages: Vec<ChatMessage> = vec![ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(system_prompt),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }];

        let mut first_user_msg_idx = None;
        for (i, msg) in self.session.log.entries().iter().enumerate() {
            let agent_msg = self.session.messages.get(i);
            let content = if msg.role == "user" && agent_msg.and_then(|m| m.images.as_ref()).is_some() && first_user_msg_idx.is_none() {
                // 只有第一条带图的 user 消息才嵌入 base64，后续迭代不再重传
                first_user_msg_idx = Some(i);
                let imgs = agent_msg.and_then(|m| m.images.as_ref()).unwrap();
                if imgs.is_empty() {
                    serde_json::Value::String(msg.content.clone())
                } else {
                    let mut parts: Vec<serde_json::Value> = vec![serde_json::json!({
                        "type": "text",
                        "text": &msg.content
                    })];
                    for url in imgs {
                        parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": { "url": url }
                        }));
                    }
                    serde_json::Value::Array(parts)
                }
            } else {
                serde_json::Value::String(msg.content.clone())
            };
            messages.push(ChatMessage {
                role: msg.role.clone(),
                content,
                tool_calls: msg.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| crate::llm::types::ToolCall {
                            id: tc.id.clone(),
                            call_type: "function".to_string(),
                            function: crate::llm::types::FunctionCall {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect()
                }),
                tool_call_id: msg.tool_call_id.clone(),
                name: msg.name.clone(),
                // DeepSeek thinking mode 必须回传 reasoning_content，否则 API 返回 400
                reasoning_content: msg.reasoning_content.clone(),
            });
        }

        // ── P0: 将 volatile 后缀追加到最后一个 user 消息末尾 ──
        // 这样 `frozen_system + history` 整体保持稳定，缓存前缀不受 volatile 影响
        if let Some(ref volatile) = self.volatile_suffix {
            if let Some(last_msg) = messages.last_mut() {
                if last_msg.role == "user" {
                    if let serde_json::Value::String(ref content) = last_msg.content {
                        let augmented = format!("{}\n\n---\n## Context & Environment\n\n{}", content, volatile);
                        last_msg.content = serde_json::Value::String(augmented);
                    }
                }
            }
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
            extra_body: Self::build_extra_body(&self.session.model),
        };

        tracing::info!(
            "[Agent] 发送 LLM 请求（{} 工具，{} 历史消息）",
            tool_count,
            self.session.messages.len()
        );

        // ── 预检：序列化请求体字节大小检查 ──
        if let Ok(body_bytes) = serde_json::to_vec(&request) {
            let body_len = body_bytes.len();
            if body_len > Self::PREFLIGHT_BODY_BYTE_HARD_LIMIT {
                tracing::warn!(
                    "[Preflight] 🔴 请求体大小 {} bytes 超过硬限制 {}，强制压缩后由重试层重试",
                    body_len,
                    Self::PREFLIGHT_BODY_BYTE_HARD_LIMIT
                );
                let hard_keep = (COMPACT_KEEP_LAST_FALLBACK / 3).max(3);
                self.session.compact_in_place(hard_keep, None);
                return Err(AppError::LlmError(format!(
                    "请求体大小 {} bytes 超过硬限制 {}，已压缩，请重试",
                    body_len, Self::PREFLIGHT_BODY_BYTE_HARD_LIMIT
                )));
            }
            if body_len > Self::PREFLIGHT_BODY_BYTE_HARD_LIMIT / 2 {
                tracing::debug!(
                    "[Preflight] 请求体大小: {} bytes (限制: {})",
                    body_len,
                    Self::PREFLIGHT_BODY_BYTE_HARD_LIMIT
                );
            }
        }

        let cancel_flag = cancel.clone();
        let mut stream_handle = self.llm_client.chat_stream(&request, cancel.clone()).await?;

        let mut full_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut was_cancelled = false;
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let mut cached_tokens: u64 = 0;

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
                                approval: None,
                                approval_id: None,
                                cached_tokens: None,
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
                                approval: None,
                                approval_id: None,
                                cached_tokens: None,
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
                StreamEvent::Usage { prompt_tokens, completion_tokens, cached_tokens: cached } => {
                    input_tokens = prompt_tokens;
                    output_tokens = completion_tokens;
                    cached_tokens = cached;
                    tracing::debug!(
                        "[Agent] Token 用量 — 输入: {}, 输出: {}, 缓存: {}",
                        input_tokens,
                        output_tokens,
                        cached_tokens
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
            tracing::debug!(
                "[Agent] Token 用量 — 输入: {}, 输出: {}, 缓存: {}",
                input_tokens,
                output_tokens,
                cached_tokens
            );
        }

        // DeepSeek thinking mode 下模型可能只输出 reasoning_content 而没有 text delta，
        // 导致 full_content 为空但 accumulated_reasoning 有实际回复内容。
        // 此时将 reasoning 内容提升为实际回复，确保前端显示正确的响应文本。
        if full_content.is_empty() && !accumulated_reasoning.is_empty() {
            tracing::debug!(
                "[Agent] full_content 为空，使用 reasoning_content 作为回复 ({} 字符)",
                accumulated_reasoning.len()
            );
            full_content = accumulated_reasoning.clone();
        }

        // ── Scavenge：从推理内容中提取被 DeepSeek 模型嵌入的工具调用 ──
        if !accumulated_reasoning.is_empty() {
            let scavenged = super::repair::scavenge(&accumulated_reasoning);
            if !scavenged.is_empty() {
                tracing::info!(
                    "[Agent] Scavenge 从推理内容中提取了 {} 个工具调用",
                    scavenged.len()
                );
                for tc in &scavenged {
                    if !accumulated_tool_calls.iter().any(|e| e.name == tc.name && e.arguments == tc.arguments) {
                        accumulated_tool_calls.push(tc.clone());
                    }
                }
            }
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
                            approval: None,
                            approval_id: None,
                            cached_tokens: None,
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
                            approval: None,
                            approval_id: None,
                            cached_tokens: None,
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
            images: None,
        };

        Ok((agent_msg, reasoning_blocks, was_cancelled, input_tokens, output_tokens, cached_tokens))
    }

    /// 构建冻结的 system prompt（仅首次运行）
    async fn build_frozen_system_prompt(&self) -> String {
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

        // 从全局状态获取 SoulManager（冻结部分需要 soul 身份）
        let soul_manager = {
            let state = crate::APP_STATE.read().await;
            state.soul_manager.clone()
        };

        crate::agent::prompt::SystemPromptBuilder::new(
            &self.config,
            os_name,
            self.session.workspace.as_deref(),
        )
        .with_soul_manager(soul_manager)
        .with_skills(Vec::new()) // skills 放入 volatile 部分
        .build_frozen()          // ⚠️ 只构建冻结部分
        .await
    }

    /// 查找可用的 flash 模型（用于摘要等辅助任务），找不到则返回 None
    /// 检查逻辑: 模型名包含 "flash"（不区分大小写）
    fn find_flash_model(&self) -> Option<String> {
        // 优先从 APP_STATE 中查找
        let state = crate::APP_STATE.try_read().ok()?;
        for provider in &state.models_config.providers {
            for model in &provider.models {
                if model.to_lowercase().contains("flash") {
                    return Some(model.clone());
                }
            }
        }
        None
    }

    /// 查找可用的 Pro 模型（静态版本，用于构造函数）
    fn find_pro_model_static() -> Option<String> {
        let state = crate::APP_STATE.try_read().ok()?;
        for provider in &state.models_config.providers {
            for model in &provider.models {
                let m = model.to_lowercase();
                if m.contains("pro") && !m.contains("flash") {
                    return Some(model.clone());
                }
            }
        }
        None
    }

    /// 生成 AI 摘要（用于上下文压缩时的语义摘要）
    /// 优先使用 flash 模型，降级到当前会话模型
    /// 失败时返回 None（调用方会使用简单的占位符）
    async fn generate_ai_summary(&self, keep_last: usize) -> Option<String> {
        // 需要至少 4 条历史消息（除了前 2 条 + 要保留的后 keep_last 条之外还有内容）
        let to_compress_end = self.session.messages.len().saturating_sub(keep_last);
        if to_compress_end <= 3 {
            return None;
        }

        // 前 2 条是系统上下文，skip 掉
        let target_messages: Vec<&crate::agent::session::AgentMessage> = self.session.messages[2..to_compress_end]
            .iter()
            .filter(|m| m.role != "system") // 跳过 system 角色
            .collect();

        if target_messages.is_empty() {
            return None;
        }

        // 如果待摘要消息太多，只取最近的部分（避免摘要本身的 token 消耗过大）
        let start_idx = if target_messages.len() > 30 {
            target_messages.len() - 30
        } else {
            0
        };
        let messages_to_summarize: Vec<&crate::agent::session::AgentMessage> =
            target_messages[start_idx..].to_vec();

        let msg_count = messages_to_summarize.len();

        // 格式化为可读文本
        let mut formatted = String::new();
        for msg in &messages_to_summarize {
            let role_tag = match msg.role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                "tool" => "  [Tool Result]",
                _ => &msg.role,
            };
            let content_preview: String = if msg.content.len() > 500 {
                // 工具结果截断
                if msg.role == "tool" {
                    let preview: String = msg.content.chars().take(500).collect();
                    // 保留末尾标记
                    if msg.tool_name.is_some() {
                        formatted.push_str(&format!("{} ({}): {}...\n", role_tag, msg.tool_name.as_ref().unwrap(), preview));
                    } else {
                        formatted.push_str(&format!("{}: {}...\n", role_tag, preview));
                    }
                    continue;
                }
                // 安全截断：按字符边界取前 500 个字符，避免 UTF-8 切片恐慌
                crate::utils::safe_truncate(&msg.content, 500)
            } else {
                msg.content.clone()
            };

            if msg.role == "tool" {
                if let Some(ref name) = msg.tool_name {
                    formatted.push_str(&format!("  [Tool: {}]\n{}\n", name, content_preview));
                } else {
                    formatted.push_str(&format!("  [Tool Result]\n{}\n", content_preview));
                }
            } else {
                formatted.push_str(&format!("{}: {}\n", role_tag, content_preview));
            }

            // 限制总输入，避免摘要 LLM 调用本身消耗过大 token
            if formatted.len() > 4000 {
                formatted.push_str("...(历史记录截断)");
                break;
            }
        }

        if formatted.trim().is_empty() {
            return None;
        }

        // 确定使用哪个模型
        let flash_model = self.find_flash_model();
        let use_model = flash_model.as_deref().unwrap_or(&self.session.model);
        let is_flash = flash_model.is_some();

        // 构建摘要 prompt
        let system_msg = crate::llm::types::ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(
                "You are a conversation summarizer. Create a concise but informative summary of the conversation history below. \
                Focus on preserving actionable information: the main goal/task, key decisions, important findings, errors/blockers, \
                what was completed vs in progress, and any code changes or configurations. \
                Output ONLY the summary, no preamble or explanation. \
                Use plain text, not markdown. Keep it under 300 words."
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };

        let user_msg = crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: serde_json::Value::String(format!(
                "Conversation history to summarize:\n\n{}", formatted
            )),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };

        let summary_request = crate::llm::types::ChatRequest {
            model: use_model.to_string(),
            messages: vec![system_msg, user_msg],
            temperature: Some(0.3),
            stream: false,
            tools: None,
            stream_options: None,
            extra_body: Self::build_extra_body(use_model),
        };

        tracing::info!(
            "[Summary] 生成 AI 摘要: 模型={}{}, 待摘要消息={}, 输入长度={}",
            use_model,
            if is_flash { " (flash)" } else { " (降级/默认)" },
            msg_count,
            formatted.len()
        );

        // 调用 LLM（非流式）
        match self.llm_client.chat(&summary_request).await {
            Ok(resp) => {
                let summary = resp.choices
                    .first()
                    .and_then(|c| c.message.as_ref())
                    .and_then(|m| m.content.as_ref())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());

                if let Some(ref s) = summary {
                    tracing::info!("[Summary] 摘要生成成功: {} 字符", s.len());
                } else {
                    tracing::warn!("[Summary] LLM 返回了空的摘要内容");
                }
                summary
            }
            Err(e) => {
                tracing::warn!("[Summary] 摘要生成失败 (将使用占位符): {}", e);
                None
            }
        }
    }

    /// 构建易变后缀（含 memory、日期、环境、技能）
    async fn build_volatile_suffix(&self) -> String {
        // 如果使用了 system_prompt_override，冻结部分已含所有内容
        if self.session.system_prompt_override.is_some() {
            return String::new();
        }

        let os_name = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };

        let (memory_content, skills) = {
            let state = crate::APP_STATE.read().await;
            let memory = state.memory_store.list_memories();
            let mem = if memory.is_empty() { None } else { Some(memory) };
            let skills: Vec<String> = self.skills.iter().map(|s| {
                format!("{}: {}", s.name, s.description)
            }).collect();
            (mem, skills)
        };

        let builder = crate::agent::prompt::SystemPromptBuilder::new(
            &self.config,
            os_name,
            self.session.workspace.as_deref(),
        )
        .with_skills(skills)
        .with_memory(memory_content);

        builder.build_volatile()
    }

    /// 机械截断：丢弃最早的一对 user+assistant 消息
    #[allow(dead_code)]
    fn truncate_messages_for_request(&mut self) {
        if self.session.messages.len() <= 4 {
            // 太短了无法截断
            return;
        }
        // 跳过前 2 条（系统上下文），找到第一对 user+assistant
        let mut remove_up_to = 2usize;
        for i in 2..self.session.messages.len() - 2 {
            if self.session.messages[i].role == "user" {
                remove_up_to = i + 1;
                // 看下一条是否是 assistant
                if i + 1 < self.session.messages.len() && self.session.messages[i + 1].role == "assistant" {
                    remove_up_to = i + 2;
                }
                break;
            }
        }
        let removed = remove_up_to - 2;
        self.session.messages.drain(2..remove_up_to);
        tracing::warn!(
            "[Preflight] 机械截断：移除了 {} 条消息，剩余 {} 条",
            removed,
            self.session.messages.len()
        );
    }

    /// 估算当前模型上下文窗口大小（基于模型名称匹配）
    /// 构建 LLM 请求的 extra_body（供应商特定参数）
    ///
    /// - DeepSeek: 设置 thinking mode 以启用/禁用思考模式
    fn build_extra_body(model_name: &str) -> Option<serde_json::Value> {
        if let Some(thinking) = deepseek_template::thinking_mode_for_model(model_name) {
            return Some(serde_json::json!({
                "thinking": {
                    "type": thinking
                }
            }));
        }
        None
    }

    fn estimate_context_window(model_name: &str) -> u64 {
        let m = model_name.to_lowercase();
        if m.contains("deepseek") {
            if m.contains("v4") || m.contains("reasoner") || m.contains("chat") || m.contains("coder") || m.contains("r1") {
                return 1_000_000;
            }
        }
        if m.contains("gpt-4") || m.contains("gpt-3.5") {
            return 128_000;
        }
        if m.contains("claude") {
            return 200_000;
        }
        128_000
    }

    /// 预检：多级上下文检查，在每次 LLM 调用前执行
    ///
    /// - Level 1 (≥70%): 日志告警
    /// - Level 2 (≥85%): 触发上下文压缩（含 AI 语义摘要）
    /// - Level 3 (≥95%): 强制压缩 + 截断（保留更少消息，含 AI 语义摘要）
    async fn maybe_compact_for_preflight(&mut self) {
        // 使用 tokenizer 精确估算 Token 数
        let total_chars: usize = self.session.messages.iter()
            .map(|m| m.content.len() + 100)
            .sum();
        let estimated_tokens: u64 = self.session.messages.iter()
            .map(|m| tokenizer::quick_estimate_message_tokens(&m.content, &m.role))
            .sum();
        let context_window = Self::estimate_context_window(&self.session.model);

        let ratio = if context_window > 0 {
            estimated_tokens as f64 / context_window as f64
        } else {
            0.0
        };

        // Level 1: 告警
        if ratio >= Self::PREFLIGHT_LEVEL1_RATIO {
            tracing::warn!(
                "[Preflight] ⚠️ 上下文使用率 {:.1}% (估算 {} tokens / {} 窗口), 模型: {}",
                ratio * 100.0,
                estimated_tokens,
                context_window,
                self.session.model
            );
        }

        // Level 2: 触发压缩（含 AI 语义摘要，保留对话上下文）
        if ratio >= Self::PREFLIGHT_LEVEL2_RATIO || total_chars > Self::PREFLIGHT_CHAR_LIMIT {
            let keep = if self.config.compact_keep > 0 { self.config.compact_keep } else { COMPACT_KEEP_LAST_FALLBACK };
            tracing::warn!(
                "[Preflight] 🔄 触发上下文压缩 (ratio={:.1}%, chars={}), 保留最近 {} 条",
                ratio * 100.0, total_chars, keep
            );
            let ai_summary = self.generate_ai_summary(keep).await;
            self.session.compact_in_place(keep, ai_summary);
        }

        // Level 3: 强制截断（保留更少消息，含 AI 语义摘要）
        if ratio >= Self::PREFLIGHT_LEVEL3_RATIO {
            let hard_keep = (COMPACT_KEEP_LAST_FALLBACK / 2).max(5);
            tracing::warn!(
                "[Preflight] 🔴 上下文严重超限 (ratio={:.1}%), 强制截断到 {} 条",
                ratio * 100.0, hard_keep
            );
            let ai_summary = self.generate_ai_summary(hard_keep).await;
            self.session.compact_in_place(hard_keep, ai_summary);
        }
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }
}