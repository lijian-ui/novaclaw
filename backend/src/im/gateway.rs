//! IM 网关
//!
//! 统一管理所有 IM 账号适配器，提供消息路由和 Agent 对接能力。
//! process_incoming_loop 实现了完整的 IM → Agent 消息路由闭环：
//!   收到消息 → 查找/创建会话 → 注入平台上下文 → Agent 处理 → 回复
//! 支持多账号：每个消息携带 account_id，按账号精确路由。

use crate::error::AppError;
use crate::im::registry::{AccountInfo, AccountRegistry};
use crate::im::session::{self as im_session, IMSessionManager};
use crate::im::types::{IncomingMessage, MessageTarget, PlatformType, SendResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

/// IM 网关
pub struct IMGateway {
    pub registry: AccountRegistry,
    pub incoming_tx: mpsc::UnboundedSender<IncomingMessage>,
}

impl IMGateway {
    /// 创建网关并启动后台入站消息处理循环
    pub fn new() -> Arc<Self> {
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();

        let gateway = Arc::new(Self {
            registry: AccountRegistry::new(),
            incoming_tx,
        });

        let gw = gateway.clone();
        tokio::spawn(async move {
            gw.process_incoming_loop(incoming_rx).await;
        });

        gateway
    }

    // ─── 注册 ───────────────────────────────────────

    pub async fn register(&self, info: AccountInfo) {
        self.registry.register(info).await;
    }

    // ─── 消息发送（按 account_id 精确路由）────────────

    /// 通过指定账号发送文本
    pub async fn send_text_by_account(
        &self,
        account_id: &str,
        target: &MessageTarget,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let composite_key = format!("{}:{}", target.platform.as_str(), account_id);
        let adapter = self
            .registry
            .get(&composite_key)
            .await
            .ok_or_else(|| AppError::NotFound(format!("账号未注册: {}", account_id)))?;
        adapter.send_text(target, text).await
    }

    /// 通过指定账号发送 Markdown
    pub async fn send_markdown_by_account(
        &self,
        account_id: &str,
        target: &MessageTarget,
        title: &str,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let composite_key = format!("{}:{}", target.platform.as_str(), account_id);
        let adapter = self
            .registry
            .get(&composite_key)
            .await
            .ok_or_else(|| AppError::NotFound(format!("账号未注册: {}", account_id)))?;
        adapter.send_markdown(target, title, text).await
    }

    /// 按消息来源账号回复
    pub async fn reply(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let adapter = self
            .registry
            .get_account(&original.account_id, &original.platform)
            .await
            .ok_or_else(|| AppError::NotFound(format!("来源账号未注册: {} (platform={})", original.account_id, original.platform)))?;
        adapter.reply(original, text).await
    }

    // ─── 查询 ───────────────────────────────────────

    pub async fn is_account_connected(&self, account_id: &str, platform: &PlatformType) -> bool {
        self.registry.is_connected(account_id, platform).await
    }

    pub async fn account_ids(&self) -> Vec<String> {
        self.registry.account_ids().await
    }

    pub async fn adapter_count(&self) -> usize {
        self.registry.len().await
    }

    // ─── 内部：入站消息处理循环 ──────────────────────

    async fn process_incoming_loop(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<IncomingMessage>) {
        tracing::info!("IMGateway 入站消息处理器已启动");

        // 初始化会话管理器（使用 Arc 共享，支持跨任务访问）
        let sessions_dir = crate::config::get_sessions_dir();
        let session_store = crate::storage::SessionStore::new(&sessions_dir);
        let session_mgr = Arc::new(IMSessionManager::new(session_store));

        // 消息缓冲区：同一会话短时间内到达的多条消息合并为一条再处理
        // iLink Bot 会将文本和媒体拆分为多条独立消息推送
        let pending: Arc<Mutex<HashMap<String, Vec<IncomingMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        while let Some(msg) = rx.recv().await {
            // 群聊：检查是否需要响应
            if !im_session::should_respond_in_group(&msg) {
                tracing::debug!("群聊消息未提及机器人，跳过: {}", msg.id);
                continue;
            }

            let sender = msg.sender_name.as_deref().unwrap_or("?").to_string();
            tracing::info!(
                "[{}] IM → Agent: [{}] {}: {}",
                msg.account_id,
                msg.platform,
                sender,
                &crate::utils::safe_truncate(&msg.text, 80),
            );

            // 计算会话唯一 key（用于消息合并）
            let source = im_session::session_source_from_incoming(&msg);
            let session_key = source.to_string();

            // 将消息加入缓冲区，判断是否为首条（决定是否启动延迟处理任务）
            let is_first = {
                let mut buf = pending.lock().await;
                let entry = buf.entry(session_key.clone()).or_default();
                let is_first = entry.is_empty();
                entry.push(msg);
                is_first
            };

            if is_first {
                // 首条消息：启动延迟处理任务，等待短时间内同会话的后续消息合并
                let gateway = self.clone();
                let mgr = session_mgr.clone();
                let key = session_key.clone();
                let buf = pending.clone();
                tokio::spawn(async move {
                    // 等待 300ms 收集同会话的后续消息
                    tokio::time::sleep(Duration::from_millis(300)).await;

                    // 取出缓冲区内所有消息
                    let merged = {
                        let mut map = buf.lock().await;
                        map.remove(&key)
                    };

                    if let Some(msgs) = merged {
                        if msgs.is_empty() {
                            return;
                        }

                        // 多条消息合并为一条（含文本、图片、视频、附件）
                        let combined = if msgs.len() == 1 {
                            msgs.into_iter().next().unwrap()
                        } else {
                            merge_messages(msgs)
                        };

                        tracing::info!(
                            "[Gateway] 合并消息: text_prefix={}, media_urls={}, video_data_urls={}, attachments={}",
                            crate::utils::safe_truncate(&combined.text, 50),
                            combined.media_urls.len(),
                            combined.video_data_urls.len(),
                            combined.attachments.len(),
                        );

                        if let Err(e) = gateway.process_single_message(&mgr, combined).await {
                            tracing::error!("处理 IM 消息失败: {}", e);
                        }
                    }
                });
            }
        }

        tracing::warn!("IMGateway 入站消息处理器已停止");
    }

    /// 处理单条入站消息：会话 → Agent → 回复
    async fn process_single_message(
        &self,
        session_mgr: &Arc<IMSessionManager>,
        msg: IncomingMessage,
    ) -> Result<(), AppError> {
        use crate::agent::runtime::AgentRuntime;
        use crate::llm::client::LlmClient;
        use crate::tools::types::AgentStep;
        use tokio::sync::mpsc;
        use std::sync::Arc;

        let source = im_session::session_source_from_incoming(&msg);

        // 1. 获取或创建 Agent 会话
        let session = session_mgr.get_or_create(&source, &msg).await?;
        let session_id = session.id.clone();

        // 1.1 加锁：防止同一会话并发处理（处理群聊多用户同时 @ 的撞车问题）
        let lock = session_mgr.get_session_lock(&session_id).await;
        let _guard = lock.lock().await;

        // 2. 格式化用户消息（注入平台上下文）

        let mut user_text = im_session::format_im_message(&msg);

        // 2.1 处理附件：保存到磁盘并追加路径到消息文本
        if !msg.attachments.is_empty() {
            let inbound_dir = crate::config::get_media_inbound_dir(&session_id);
            if let Err(e) = std::fs::create_dir_all(&inbound_dir) {
                tracing::warn!("[Gateway] 创建附件目录失败: {}", e);
            } else {
                let mut path_texts: Vec<String> = Vec::new();
                for attachment in &msg.attachments {
                    let uuid = Uuid::new_v4();
                    let safe_name = format!("{}_{}", uuid, attachment.file_name);
                    let file_path = inbound_dir.join(&safe_name);
                    match std::fs::write(&file_path, &attachment.data) {
                        Ok(_) => {
                            let path_str = file_path.to_string_lossy().to_string();
                            tracing::info!("[Gateway] 附件已保存: {}", path_str);
                            // 视频类附件已通过 video_data_urls 传递给多模态 LLM，
                            // 不再追加路径文本到 user_text，避免 LLM 尝试用系统工具查看视频文件
                            if !attachment.mime_type.starts_with("video/") {
                                path_texts.push(format!("[附件: {} -> {}]", attachment.file_name, path_str));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[Gateway] 保存附件失败 ({}): {}", safe_name, e);
                        }
                    }
                }
                if !path_texts.is_empty() {
                    let attachment_text = path_texts.join("\n");
                    if user_text.is_empty() {
                        user_text = attachment_text;
                    } else {
                        user_text = format!("{}\n\n{}", user_text, attachment_text);
                    }
                }
            }
        }

        // 日志：记录 media_urls 信息（图片调试用）
        tracing::info!(
            "[Gateway] 处理消息: text_prefix={}, session_id={}, media_urls数量={}, 首条前缀={}",
            crate::utils::safe_truncate(&msg.text, 50),
            session_id,
            msg.media_urls.len(),
            msg.media_urls.first().map(|u| crate::utils::safe_truncate(u, 60)).unwrap_or("(无)".to_string())
        );

        // 3. 获取 LLM 客户端的配置
        let (provider, config, tool_registry, models_config, skills) = {
            let state = crate::APP_STATE.read().await;
            // 始终使用当前默认模型，而非会话创建时的旧模型
            let model = state.models_config.default_model.clone();
            let provider = state
                .models_config
                .find_provider_by_model(&model)
                .ok_or_else(|| {
                    AppError::NotFound(format!("模型 {} 未配置提供商", model))
                })?
                .clone();
            let app_config = state.config.clone();
            let registry = Arc::new(state.tool_registry.clone());
            let skill_list = crate::skills::loader::SkillsLoader::filter_enabled(
                state.skills_loader.list_skills(),
                &state.config.skills,
            );
            let mm_config = state.models_config.clone();
            (provider, app_config, registry, mm_config, skill_list)
        };

        // 4. 创建 LLM 客户端和 Agent Runtime
        let llm_client = LlmClient::new(provider, config.llm_timeout)
            .map_err(|e| AppError::Internal(format!("创建 LLM 客户端失败: {}", e)))?;
        let mut runtime = AgentRuntime::new(
            session,
            llm_client,
            tool_registry,
            &config,
            models_config,
            skills,
        );
        // 注入 IM 回复上下文（告知 LLM 如何通过 im_push 回复）
        runtime.im_reply_context = Some(im_session::build_im_reply_context(&msg));

        // 5. 尝试启动流式回复（仅 DingTalk 支持 AI Card 流式）
        let (stream_tx, is_streaming) = {
            let adapter = self.registry.get_account(&msg.account_id, &msg.platform).await;
            match adapter {
                Some(adap) => {
                    match adap.start_stream_reply(&msg).await {
                        Ok(tx) => (Some(tx), true),
                        Err(_) => (None, false),
                    }
                }
                None => (None, false),
            }
        };

        // 6. 创建 AgentStep 通道，在 run_turn 之前启动监听
        let (step_tx, step_rx) = if is_streaming {
            let (tx, rx) = mpsc::channel::<AgentStep>(32);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // 如果启用流式，提前启动监听任务（在 run_turn 之前）
        if let (Some(tx), Some(mut rx)) = (stream_tx, step_rx) {
            tokio::spawn(async move {
                while let Some(step) = rx.recv().await {
                    if step.step_type == "text_chunk" {
                        let _ = tx.send(step.content);
                    }
                }
                tracing::info!("IM 流式回复完成");
            });
        }

        // 7. 执行 Agent（传入图片/视频 Base64 data URL 以支持多模态识别）
        if !msg.video_data_urls.is_empty() {
            // 视频消息：先回复"正在下载"，后台异步处理
            let _ = self.reply(&msg, "收到视频，正在下载并处理中，请稍候...").await;
        }
        let result = runtime.run_turn(&user_text, step_tx, None, &msg.media_urls, &msg.video_data_urls).await?;
        let reply_content = result.content.trim().to_string();

        // 8. 持久化会话消息
        {
            // 8a. 将图片 Base64 data URL 保存到磁盘，获得文件路径列表
            let image_paths: Option<Vec<String>> = {
                if msg.media_urls.is_empty() {
                    None
                } else {
                    let paths: Vec<String> = msg.media_urls
                        .iter()
                        .filter_map(|url| {
                            match crate::server::routes::chat::save_image_data_url(url, &result.session_id) {
                                Ok(filename) => {
                                    tracing::info!("[Gateway] 图片已保存: {}/{}", result.session_id, filename);
                                    Some(filename)
                                }
                                Err(e) => {
                                    tracing::warn!("[Gateway] 保存图片失败: {}", e);
                                    None
                                }
                            }
                        })
                        .collect();
                    if paths.is_empty() { None } else { Some(paths) }
                }
            };

            let _ = crate::APP_STATE
                .read()
                .await
                .session_store
                .append_message(&result.session_id, &crate::storage::Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: result.session_id.clone(),
                    role: "user".to_string(),
                    content: user_text.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    metadata: None,
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    first_reasoning: None,
                    again_reasonings: None,
                    reasoning: None,
                    input_tokens: None,
                    output_tokens: None,
                    cached_tokens: None,
                    last_input_tokens: None,
                    last_output_tokens: None,
                    image_paths: image_paths.clone(),
                    message_type: None,
                    cache_hit_rate: None,
                });
            let _ = crate::APP_STATE
                .read()
                .await
                .session_store
                .append_message(&result.session_id, &crate::storage::Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: result.session_id.clone(),
                    role: "assistant".to_string(),
                    content: reply_content.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    metadata: None,
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    first_reasoning: None,
                    again_reasonings: None,
                    reasoning: None,
                    input_tokens: Some(result.total_input_tokens),
                    output_tokens: Some(result.total_output_tokens),
                    cached_tokens: Some(result.total_cached_tokens),
                    last_input_tokens: Some(result.last_input_tokens),
                    last_output_tokens: Some(result.last_output_tokens),
                    image_paths: None,
                    message_type: None,
                    cache_hit_rate: Some(result.cache_hit_rate),
                });
        }

        // 9. 发送回复到 IM 平台（非流式路径或流式已完成文本推送）
        if is_streaming {
            // 流式模式下，等待卡片后台任务完成最终内容
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if let Some(adapter) = self.registry.get_account(&msg.account_id, &msg.platform).await {
                let _ = adapter.finish_stream_reply(&msg).await;
            }
        } else if reply_content.is_empty() {
            tracing::warn!("Agent 返回空回复 (session={})", result.session_id);
            self.reply(&msg, "抱歉，我没有生成有效的回复，请重试。")
                .await
                .ok();
        } else {
            tracing::info!(
                "Agent → IM: {}，长度={}字符",
                &crate::utils::safe_truncate(&reply_content, 50),
                reply_content.len()
            );
            self.reply(&msg, &reply_content).await?;
        }

        Ok(())
    }
}

/// 合并同一会话短时间内到达的多条消息为一条。
///
/// iLink Bot 等 SDK 会将文本和媒体拆分为多条独立消息推送，
/// 此函数将它们合并为一条 IncomingMessage，确保 Agent 能同时看到文本和媒体内容。
fn merge_messages(msgs: Vec<IncomingMessage>) -> IncomingMessage {
    let mut iter = msgs.into_iter();
    let mut base = iter.next().expect("merge_messages: empty input");

    for msg in iter {
        // 合并文本（非空且不重复则追加）
        if !msg.text.is_empty() && !base.text.contains(&msg.text) {
            if base.text.is_empty() {
                base.text = msg.text;
            } else {
                base.text.push('\n');
                base.text.push_str(&msg.text);
            }
        }
        // 合并媒体资源
        base.media_urls.extend(msg.media_urls);
        base.video_data_urls.extend(msg.video_data_urls);
        base.attachments.extend(msg.attachments);
        // 保留最后的时间戳
        base.timestamp = base.timestamp.max(msg.timestamp);
        // 保留最后一条的 raw（更完整）
        base.raw = msg.raw;
        // 保留非空的 session_webhook
        if msg.session_webhook.is_some() {
            base.session_webhook = msg.session_webhook;
        }
        // 保留非空的 sender_name
        if msg.sender_name.is_some() {
            base.sender_name = msg.sender_name;
        }
    }

    base
}
