use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::log::{AppendOnlyLog, LogEntry};

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
    /// frozen_system_prompt 的 SHA256 指纹（前 16 字符 hex），用于检测缓存漂移
    pub frozen_prefix_fingerprint: Option<String>,
    /// 前缀是否已失效（指纹变化时设为 true），需要外部消费方据此刷新
    pub prefix_invalidated: bool,
    /// 对话消息历史（仅追加，除 compact_in_place 外不得原地修改）
    pub messages: Vec<AgentMessage>,
    /// 追加确定性日志（与 messages 同步，用于 LLM 请求序列化）
    #[serde(skip)]
    pub log: AppendOnlyLog,
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
    /// 固定到上下文的文件（path -> content）
    pub pinned_files: HashMap<String, String>,
    /// 连续读取文件计数器，用于触发警告
    pub consecutive_read_count: usize,
    /// 会话存储（实时持久化用，不序列化）
    #[serde(skip)]
    pub session_store: Option<crate::storage::SessionStore>,
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
    /// 视频 data URL 列表 (data:video/...;base64, 格式，用于多模态 LLM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub videos: Option<Vec<String>>,
    /// 消息权重：用于压缩算法优先级。包含代码变更、核心决策的消息权重较高。
    #[serde(default)]
    pub weight: u32,
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
            frozen_prefix_fingerprint: None,
            prefix_invalidated: false,
            messages: Vec::new(),
            log: AppendOnlyLog::new(),
            compaction_count: 0,
            created_at: now.clone(),
            updated_at: now,
            total_input_tokens: 0,
            total_output_tokens: 0,
            cache_hit_tokens: 0,
            cache_miss_tokens: 0,
            pinned_files: HashMap::new(),
            consecutive_read_count: 0,
            session_store: None,
        }
    }

    /// 最大消息数（滑动窗口，超限时丢弃最旧的 tool/assistant 消息）
    const MAX_MESSAGES: usize = 200;

    /// 添加消息到会话历史（仅追加！不允许原地修改已有消息）
    pub fn push_message(&mut self, msg: AgentMessage) {
        // 实时持久化 tool 消息到 JSONL（崩溃安全；放在 push 之前以避免 borrow 冲突）
        // assistant 消息由 chat.rs 在 run_turn 完成后批量写入（含 token 信息）
        if let Some(ref store) = self.session_store {
            if msg.role == "tool" {
                let tcs = msg.tool_calls.as_ref().map(|tcs| {
                    tcs.iter().map(|tc| crate::storage::ToolCall {
                        id: tc.id.clone(), name: tc.name.clone(),
                        arguments: Some(tc.arguments.clone()),
                    }).collect()
                });
                let storage_msg = crate::storage::Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: self.id.clone(),
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    metadata: None, tool_calls: tcs,
                    tool_call_id: msg.tool_call_id.clone(),
                    tool_name: msg.tool_name.clone(),
                    first_reasoning: None, again_reasonings: None,
                    reasoning: None,
                    input_tokens: None, output_tokens: None,
                    cached_tokens: None,
                    last_input_tokens: None, last_output_tokens: None,
                    cache_hit_rate: None,
                    image_paths: None, message_type: None,
                };
                let _ = store.append_message(&self.id, &storage_msg);
            }
        }
        // 同步追加到确定性日志
        let entry: LogEntry = (&msg).into();
        self.log.push(entry);
        self.messages.push(msg);
        // 滑动窗口：超过 MAX_MESSAGES 时丢弃最旧的 tool/assistant 消息（保留 system + user）
        while self.messages.len() > Self::MAX_MESSAGES {
            // 从索引 2 开始找第一条可丢弃的消息（跳过 system + user 输入）
            let drop_idx = (2..self.messages.len()).find(|&i| {
                let role = self.messages[i].role.as_str();
                role == "tool" || role == "assistant"
            });
            match drop_idx {
                Some(idx) => {
                    self.messages.remove(idx);
                }
                None => break, // 没有可丢弃的消息
            }
        }
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// 计算字符串的指纹（基于 std hash，确定性输出 16 字符 hex），用于检测缓存前缀变化
    fn compute_fingerprint(content: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// 设置冻结的系统提示词前缀，附带 SHA256 指纹检测
    ///
    /// # 返回
    /// - `Ok(true)` — 首次设置成功
    /// - `Ok(false)` — 前缀与上次一致，无变化（跳过）
    /// - `Err(fingerprint)` — 前缀已存在但指纹不匹配！说明缓存已漂移，需外部消费方处理
    pub fn set_frozen_system_prompt(&mut self, prompt: String) -> Result<bool, String> {
        let new_fingerprint = Self::compute_fingerprint(&prompt);

        let existing_fingerprint = self.frozen_prefix_fingerprint.take();

        if let Some(old_fp) = existing_fingerprint {
            if old_fp == new_fingerprint {
                // 前缀完全一致，缓存可复用
                self.frozen_prefix_fingerprint = Some(new_fingerprint.clone());
                tracing::debug!(
                    "[Cache] frozen_system_prompt 指纹匹配 ({}), 缓存前缀稳定",
                    new_fingerprint
                );
                return Ok(false);
            }
            // 指纹不匹配！说明 system_prompt_override/SOUL.md 等发生了变化
            // 缓存前缀已漂移，本次请求必定 cache miss
            tracing::warn!(
                "[Cache] ⚠️ frozen_system_prompt 指纹变化! 旧={}, 新={}, 缓存前缀已漂移，下次请求将 cache miss",
                old_fp, new_fingerprint
            );
            self.frozen_system_prompt = Some(prompt);
            self.frozen_prefix_fingerprint = Some(new_fingerprint.clone());
            self.prefix_invalidated = true;
            return Err(format!(
                "缓存前缀指纹变化: {} → {}, 前缀已更新",
                old_fp, new_fingerprint
            ));
        }

        // 首次设置
        tracing::info!(
            "[Cache] frozen_system_prompt 首次设置 ({} 字符, 指纹: {})",
            prompt.len(),
            new_fingerprint
        );
        self.frozen_system_prompt = Some(prompt);
        self.frozen_prefix_fingerprint = Some(new_fingerprint);
        Ok(true)
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

    /// 固定文件到上下文
    pub fn pin_file(&mut self, path: String, content: String) {
        self.pinned_files.insert(path, content);
        self.prefix_invalidated = true; // 钉住文件会改变前缀，标记失效
    }

    /// 取消固定文件
    pub fn unpin_file(&mut self, path: &str) {
        if self.pinned_files.remove(path).is_some() {
            self.prefix_invalidated = true;
        }
    }

    /// 获取所有固定文件的格式化字符串
    pub fn get_pinned_files_context(&self) -> String {
        if self.pinned_files.is_empty() {
            return String::new();
        }
        let mut context = String::from("\n# Pinned Context (Files in focus)\n\n");
        for (path, content) in &self.pinned_files {
            context.push_str(&format!("## File: {}\n```\n{}\n```\n\n", path, content));
        }
        context
    }

    /// 激进地压缩工具结果
    pub fn aggressive_compact_tool_results(&mut self, keep_recent: usize) {
        let mut tool_results_indices = Vec::new();
        for (i, msg) in self.messages.iter().enumerate() {
            if msg.role == "tool" {
                tool_results_indices.push(i);
            }
        }

        if tool_results_indices.len() <= keep_recent {
            return;
        }

        let to_compact = &tool_results_indices[..tool_results_indices.len() - keep_recent];
        for &idx in to_compact {
            let content = &self.messages[idx].content;
            // 如果是 read_file 的结果且内容较长，进行截断并替换为摘要
            if content.contains("read_file") && content.len() > 500 {
                let summary = format!("[已激进压缩] 历史文件读取内容已移除。原长度: {} 字符。如需再次查看，请重新读取或将其 PIN 到上下文。", content.len());
                self.messages[idx].content = summary;
            }
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
            videos: None,
            weight: 0,
        });

    }

    /// 添加用户消息（含视频 data URL）
    pub fn push_user_with_videos(&mut self, content: &str, video_urls: &[String]) {
        self.push_message(AgentMessage {
            role: "user".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            images: None,
            videos: if video_urls.is_empty() { None } else { Some(video_urls.to_vec()) },
            weight: 0,
        });
    }

    /// 添加用户消息（含图片和视频 data URL）
    pub fn push_user_with_images_and_videos(&mut self, content: &str, image_urls: &[String], video_urls: &[String]) {
        let has_images = !image_urls.is_empty();
        let has_videos = !video_urls.is_empty();
        if has_images && has_videos {
            // 同时有图片和视频时，合并到一条消息中
            self.push_message(AgentMessage {
                role: "user".to_string(),
                content: content.to_string(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                first_reasoning: None,
                again_reasonings: None,
                reasoning: None,
                images: Some(image_urls.to_vec()),
                videos: Some(video_urls.to_vec()),
                weight: 0,
            });
        } else if has_videos {
            self.push_user_with_videos(content, video_urls);
        } else {
            self.push_user_with_images(content, image_urls);
        }
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
            videos: None,
            weight: 0,
        });

    }


    /// 添加工具结果消息
    pub fn push_tool_result(&mut self, tool_call_id: &str, tool_name: &str, output: &str) {
        // MD5 Deduplication (using DefaultHasher for efficiency)
        // If a tool returns identical output to a PREVIOUS call in the SAME session,
        // we replace it with a label to save tokens.
        let is_duplicate = if output.len() > 100 {
            self.messages.iter().rev()
                .filter(|m| m.role == "tool" && m.tool_name.as_deref() == Some(tool_name))
                .take(10) // Only check last 10 similar tool outputs
                .any(|m| m.content == output)
        } else {
            false
        };

        let content = if is_duplicate {
            format!("[DUPLICATE RESULT of {} — output identical to previous turn, hidden to save tokens]", tool_name)
        } else {
            output.to_string()
        };

        // 赋权：关键操作（写文件、应用补丁、执行命令）权重较高，防止被轻易压缩
        let weight = match tool_name {
            "apply_patch" | "write_file" | "search_replace" | "rename_file" => 80,
            "execute_command" if !content.contains("error") => 60,
            "delegate_task" => 40,
            _ => 0,
        };

        self.push_message(AgentMessage {
            role: "tool".to_string(),
            content,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            tool_name: Some(tool_name.to_string()),
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            images: None,
            videos: None,
            weight,
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
    ///
    /// # 行为
    /// - 保留前 2 条（系统上下文）
    /// - 尾部按 token 预算保护（上下文窗口的 20%），而非固定消息条数
    /// - 中间旧消息被一条摘要消息替换，并附带工具执行记录
    /// - `ai_summary` 为 None 时使用简单的计数占位符；为 Some 时使用 LLM 生成的语义摘要
    /// - 摘要消息 role 设置为 `assistant`（更符合对话语境，防止与 system prompt 混淆）
    /// - 从 frozen_system_prompt 中提取约束条件（do NOT / never / avoid），钉住到摘要末尾
    pub fn compact_in_place(&mut self, keep_last: usize, ai_summary: Option<String>) {
        // 尾部预算：上下文窗口的 20%（参考 Reasonix HISTORY_FOLD_TAIL_FRACTION）
        // 根据模型名称估算上下文窗口大小
        let ctx_window = Self::estimate_context_window_for_model(&self.model);
        let tail_token_budget: u64 = (ctx_window as f64 * 0.20) as u64;
        // 最少保留 3 条，最多不超过 keep_last
        const MIN_TAIL_MESSAGES: usize = 3;

        if self.messages.len() <= keep_last + 2 {
            return;
        }

        let total = self.messages.len();

        // 尾部保护：从末尾向前走，按 token 预算保留消息
        let mut tail_tokens: u64 = 0;
        let mut tail_count: usize = 0;
        for msg in self.messages.iter().rev().take(total - 2) {
            let tokens = crate::llm::tokenizer::estimate_string_tokens(&msg.content);
            let extra = match msg.role.as_str() {
                "assistant" if msg.tool_calls.is_some() => 50u64,
                "tool" => 20u64,
                _ => 10u64,
            };
            tail_tokens += tokens + extra;
            tail_count += 1;
            if tail_tokens >= tail_token_budget && tail_count >= MIN_TAIL_MESSAGES {
                break;
            }
        }
        let effective_keep = tail_count
            .min(keep_last)
            .max(MIN_TAIL_MESSAGES.min(total.saturating_sub(2)));

        // 保留前2条（系统上下文）和后 effective_keep 条（按 token 预算）
        let front: Vec<_> = self.messages.iter().take(2).cloned().collect();
        let to_remove_count = total - 2 - effective_keep;
        if to_remove_count == 0 {
            return;
        }
        let back: Vec<_> = self.messages.iter().skip(2 + to_remove_count).cloned().collect();

        // 扫描中间区域（ folding zone ），提取工具执行记录，并保留高权重消息
        let mid_msgs = &self.messages[2..2 + to_remove_count];
        let mut pinned_from_mid = Vec::new();
        let mut folded_msgs = Vec::new();

        for msg in mid_msgs {
            if msg.weight >= 50 {
                pinned_from_mid.push(msg.clone());
            } else {
                folded_msgs.push(msg);
            }
        }

        let tool_records: Vec<String> = folded_msgs.iter().filter_map(|m| {
            if m.role == "tool" {
                if let Some(ref name) = m.tool_name {
                    let preview: String = m.content.chars().take(120).collect();
                    let size = m.content.len();
                    Some(format!("  [{}.{}]: {} ({} chars)", name, m.tool_call_id.as_deref().unwrap_or("?"), preview.replace("\n", " "), size))
                } else {
                    None
                }
            } else if m.role == "assistant" {
                if let Some(ref calls) = m.tool_calls {
                    if !calls.is_empty() {
                        let names: Vec<String> = calls.iter().map(|tc| {
                            let args_preview: String = tc.arguments.chars().take(50).collect();
                            format!("{}({})", tc.name, args_preview.replace("\n", " "))
                        }).collect();
                        if !names.is_empty() {
                            Some(format!("  → 调用了: {}", names.join(", ")))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }).collect();
        let tool_section = if tool_records.is_empty() {
            String::new()
        } else {
            format!("\n\n## Tools Executed (Historical Record)\n{}", tool_records.join("\n"))
        };

        // 从 frozen_system_prompt 中提取约束条件（参考 Reasonix extractPinnedConstraints）
        // 确保 "do NOT" / "never" / "avoid" 等负向约束在折叠后不被遗忘
        let constraint_tail = self.extract_pinned_constraints();

        let summary_content = match ai_summary {
            Some(ref s) if !s.trim().is_empty() => {
                format!(
                    "[CONVERSATION HISTORY SUMMARY — earlier turns folded for context efficiency]\n\n{}{}{}",
                    s.trim(),
                    tool_section,
                    constraint_tail
                )
            }
            _ => {
                format!(
                    "[CONVERSATION HISTORY SUMMARY — removed {} historical messages, showing recent conversation content]{}{}",
                    to_remove_count,
                    tool_section,
                    constraint_tail
                )
            }
        };

        let summary = AgentMessage {
            role: "assistant".to_string(),
            content: summary_content,
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            images: None,
            videos: None,
            weight: 0,
        };

        self.messages = front;
        self.messages.push(summary);
        self.messages.extend(pinned_from_mid);
        self.messages.extend(back);

        Self::strip_orphan_tool_calls(&mut self.messages);

        self.log.clear();
        for msg in &self.messages {
            let entry: LogEntry = msg.into();
            self.log.push(entry);
        }

        self.compaction_count += 1;
        tracing::info!(
            "[Cache] compact_in_place{}: 移除了 {} 条消息，剩余 {} 条 (压缩次数: {})，尾部预算 {} tokens",
            if ai_summary.is_some() { " (AI 摘要)" } else { "" },
            to_remove_count,
            self.messages.len(),
            self.compaction_count,
            tail_token_budget,
        );
    }

    /// 根据模型名称估算上下文窗口大小（用于 compact_in_place 的尾部预算计算）
    fn estimate_context_window_for_model(model: &str) -> u64 {
        let m = model.to_lowercase();
        if m.contains("deepseek") {
            if m.contains("v4") || m.contains("reasoner") || m.contains("chat")
                || m.contains("coder") || m.contains("r1")
            {
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

    /// 从 frozen_system_prompt 中提取约束条件（参考 Reasonix extractPinnedConstraints）
    ///
    /// 提取包含 "do NOT" / "never" / "avoid" / "禁止" / "不得" 等关键词的规则段落，
    /// 钉住到折叠摘要末尾，确保 LLM 在折叠后不会遗忘负向约束。
    fn extract_pinned_constraints(&self) -> String {
        let system = match &self.frozen_system_prompt {
            Some(s) if !s.trim().is_empty() => s,
            _ => return String::new(),
        };

        // 提取 "# Rules" 或 "## Rules" 段落（最常见的约束位置）
        let mut constraints = Vec::new();

        // 按段落分割，找包含约束关键词的段落
        let constraint_keywords = [
            "do not", "do NOT", "never", "avoid", "must not", "禁止", "不得", "不要",
            "不允许", "严禁",
        ];

        for line in system.lines() {
            let line_lower = line.to_lowercase();
            if constraint_keywords.iter().any(|kw| line_lower.contains(&kw.to_lowercase())) {
                let trimmed = line.trim();
                if !trimmed.is_empty() && trimmed.len() > 5 {
                    constraints.push(trimmed.to_string());
                }
            }
        }

        if constraints.is_empty() {
            return String::new();
        }

        // 最多保留 10 条约束，避免摘要过长
        let kept: Vec<_> = constraints.into_iter().take(10).collect();
        format!(
            "\n\n[PINNED CONSTRAINTS — preserved verbatim across fold]\n{}",
            kept.iter().map(|c| format!("- {}", c)).collect::<Vec<_>>().join("\n")
        )
    }

    /// 治愈整个会话（清理孤立工具消息，同步 log）
    pub fn heal(&mut self) {
        Self::strip_orphan_tool_calls(&mut self.messages);
        
        // 同步 log
        self.log.clear();
        for msg in &self.messages {
            let entry: LogEntry = msg.into();
            self.log.push(entry);
        }
    }

    /// 扫描并清理孤立 tool_calls 和孤立的 tool 消息 — 防止违反 API 协议
    ///
    /// 严格清理规则（顺序相关）：
    /// 1. tool 消息必须紧随包含该 ID 的 assistant 消息（中间只能有其他 tool 消息）
    /// 2. assistant 消息中的 tool_calls 必须被紧随其后的 tool 消息完整响应
    /// 3. 任何不符合顺序约束的工具调用/结果都将被剥离或删除
    pub fn strip_orphan_tool_calls(messages: &mut Vec<AgentMessage>) {
        if messages.is_empty() {
            return;
        }

        let original_len = messages.len();

        // --- 第一阶段：清理孤立的 tool 消息（找不到对应的前置调用者） ---
        let mut to_remove_indices = std::collections::HashSet::new();
        for i in 0..messages.len() {
            if messages[i].role == "tool" {
                let current_id = messages[i].tool_call_id.as_deref().unwrap_or("");
                let mut found_parent = false;
                
                // 向前查找，跳过连续的 tool 消息
                for j in (0..i).rev() {
                    if messages[j].role == "tool" {
                        continue;
                    } else if messages[j].role == "assistant" {
                        if let Some(ref calls) = messages[j].tool_calls {
                            if calls.iter().any(|tc| tc.id == current_id) {
                                found_parent = true;
                            }
                        }
                        break;
                    } else {
                        // 遇到 user/system，说明 tool 消息是孤立的
                        break;
                    }
                }
                
                if !found_parent {
                    tracing::warn!("[Heal] 发现孤立工具响应 (ID: {}), 索引: {}, 已标记移除", current_id, i);
                    to_remove_indices.insert(i);
                }
            }
        }

        // 执行第一阶段删除
        let mut i = 0;
        messages.retain(|_| {
            let keep = !to_remove_indices.contains(&i);
            i += 1;
            keep
        });

        // --- 修复阶段：确保 tool 消息有 tool_call_id，如果没有则尝试恢复或删除 ---
        for msg in messages.iter_mut() {
            if msg.role == "tool" && (msg.tool_call_id.is_none() || msg.tool_call_id.as_deref().unwrap_or("").is_empty()) {
                // 尝试从 content 标签中恢复（如果存在）
                if let Some(ref name) = msg.tool_name {
                    tracing::warn!("[Heal] 发现 role='tool' 但缺失 tool_call_id 的消息 (工具: {})", name);
                }
            }
        }

        // --- 第二阶段：清理 assistant 消息中未被响应的 tool_calls ---
        for i in 0..messages.len() {
            if messages[i].role != "assistant" { continue; }
            if messages[i].tool_calls.is_none() { continue; }
            
            // 先收集响应 ID（不可变借用，不与接下来的可变借用冲突）
            let mut responded_ids = std::collections::HashSet::new();
            for j in (i + 1)..messages.len() {
                if messages[j].role == "tool" {
                    if let Some(ref id) = messages[j].tool_call_id {
                        responded_ids.insert(id.clone());
                    }
                } else { break; }
            }
            
            // 再执行清理（可变借用，与上一步不冲突）
            if let Some(ref mut calls) = messages[i].tool_calls {
                let before_count = calls.len();
                calls.retain(|tc| responded_ids.contains(&tc.id));
                if calls.len() < before_count {
                    tracing::warn!("[Heal] Assistant 消息 (索引: {}) 中有 {} 个工具调用未收到响应，已剥离", i, before_count - calls.len());
                }
                if calls.is_empty() {
                    messages[i].tool_calls = None;
                }
            }
        }

        // --- 第三阶段：确保所有剩余的 tool 消息都有 tool_call_id ---
        for msg in messages.iter_mut() {
            if msg.role == "tool" && (msg.tool_call_id.is_none() || msg.tool_call_id.as_deref().unwrap_or("").is_empty()) {
                // 这是一个严重的格式错误，API 会直接报错
                // 尝试恢复或补齐
                msg.tool_call_id = Some("recovered_id".to_string());
            }
        }

        if messages.len() < original_len {
            tracing::info!("[Heal] 会话自修复完成: 移除了 {} 条无效工具响应", original_len - messages.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_orphan_tool_calls_sequential() {
        let mut messages = vec![
            AgentMessage {
                role: "assistant".to_string(),
                content: "Call A".to_string(),
                tool_calls: Some(vec![AgentToolCall { id: "A".to_string(), name: "test".to_string(), arguments: "{}".to_string() }]),
                tool_calls: None, tool_name: None, first_reasoning: None, again_reasonings: None, reasoning: None, images: None, videos: None, weight: 0,
            },
            AgentMessage {
                role: "tool".to_string(),
                content: "Result A".to_string(),
                tool_calls: None,
                tool_call_id: Some("A".to_string()),
                tool_name: Some("test".to_string()),
                first_reasoning: None, again_reasonings: None, reasoning: None, images: None, videos: None, weight: 0,
            },
            AgentMessage {
                role: "assistant".to_string(),
                content: "Call B".to_string(),
                tool_calls: Some(vec![AgentToolCall { id: "B".to_string(), name: "test".to_string(), arguments: "{}".to_string() }]),
                tool_calls: None, tool_name: None, first_reasoning: None, again_reasonings: None, reasoning: None, images: None, videos: None, weight: 0,
            },
            AgentMessage {
                role: "user".to_string(),
                content: "Next".to_string(),
                tool_calls: None, tool_call_id: None, tool_name: None, first_reasoning: None, again_reasonings: None, reasoning: None, images: None, videos: None, weight: 0,
            },
            AgentMessage {
                role: "tool".to_string(),
                content: "Orphan Result C".to_string(),
                tool_call_id: Some("C".to_string()),
                tool_name: Some("test".to_string()),
                tool_calls: None, first_reasoning: None, again_reasonings: None, reasoning: None, images: None, videos: None, weight: 0,
            },
        ];

        AgentSession::strip_orphan_tool_calls(&mut messages);

        assert_eq!(messages.len(), 4);
        assert!(messages[0].tool_calls.is_some());
        assert_eq!(messages[1].role, "tool");
        assert!(messages[2].tool_calls.is_none());
        assert_eq!(messages[3].role, "user");
    }
}
