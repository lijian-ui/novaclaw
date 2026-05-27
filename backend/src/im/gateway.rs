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
use std::sync::Arc;
use tokio::sync::mpsc;

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

        // 初始化会话管理器
        let sessions_dir = crate::config::get_sessions_dir();
        let session_store = crate::storage::SessionStore::new(&sessions_dir);
        let session_mgr = IMSessionManager::new(session_store);

        while let Some(msg) = rx.recv().await {
            // 群聊：检查是否需要响应
            if !im_session::should_respond_in_group(&msg) {
                tracing::debug!("群聊消息未提及机器人，跳过: {}", msg.id);
                continue;
            }

            let sender = msg.sender_name.as_deref().unwrap_or("?");
            tracing::info!(
                "[{}] IM → Agent: [{}] {}: {}",
                msg.account_id,
                msg.platform,
                sender,
                &msg.text[..msg.text.len().min(80)],
            );

            // 每个消息独立 try 块，防止单条消息异常影响后续
            if let Err(e) = self
                .process_single_message(&session_mgr, msg)
                .await
            {
                tracing::error!("处理 IM 消息失败: {}", e);
            }
        }

        tracing::warn!("IMGateway 入站消息处理器已停止");
    }

    /// 处理单条入站消息：会话 → Agent → 回复
    async fn process_single_message(
        &self,
        session_mgr: &IMSessionManager,
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

        // 2. 格式化用户消息（注入平台上下文）
        let user_text = im_session::format_im_message(&msg);

        // 3. 获取 LLM 客户端的配置
        let (provider, config, tool_registry, skills) = {
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
            let skill_list = state.skills_loader.list_skills();
            (provider, app_config, registry, skill_list)
        };

        // 4. 创建 LLM 客户端和 Agent Runtime
        let llm_client = LlmClient::new(provider, config.llm_timeout)
            .map_err(|e| AppError::Internal(format!("创建 LLM 客户端失败: {}", e)))?;
        let mut runtime = AgentRuntime::new(
            session,
            llm_client,
            tool_registry,
            &config,
            skills,
        );

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

        // 7. 执行 Agent
        let result = runtime.run_turn(&user_text, step_tx, None, &[]).await?;
        let reply_content = result.content.trim().to_string();

        // 8. 持久化会话消息
        {
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
                    image_paths: None,
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
                &reply_content.chars().take(50).collect::<String>(),
                reply_content.len()
            );
            self.reply(&msg, &reply_content).await?;
        }

        Ok(())
    }
}
