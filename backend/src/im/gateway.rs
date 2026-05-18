//! IM 网关
//!
//! 统一管理所有 IM 平台适配器，提供消息路由和 Agent 对接能力。
//! process_incoming_loop 实现了完整的 IM → Agent 消息路由闭环：
//!   收到消息 → 查找/创建会话 → 注入平台上下文 → Agent 处理 → 回复

use crate::agent::runtime::AgentRuntime;
use crate::error::AppError;
use crate::im::adapter::IMAdapter;
use crate::im::registry::PlatformRegistry;
use crate::im::session::{self as im_session, IMSessionManager};
use crate::im::types::{IncomingMessage, MessageTarget, PlatformType, SendResult};
use crate::llm::client::LlmClient;
use std::sync::Arc;
use tokio::sync::mpsc;

/// IM 网关
pub struct IMGateway {
    registry: PlatformRegistry,
    pub incoming_tx: mpsc::UnboundedSender<IncomingMessage>,
}

impl IMGateway {
    /// 创建网关并启动后台入站消息处理循环
    pub fn new() -> Arc<Self> {
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();

        let gateway = Arc::new(Self {
            registry: PlatformRegistry::new(),
            incoming_tx,
        });

        let gw = gateway.clone();
        tokio::spawn(async move {
            gw.process_incoming_loop(incoming_rx).await;
        });

        gateway
    }

    // ─── 注册 ───────────────────────────────────────

    pub async fn register(&self, adapter: Arc<dyn IMAdapter>) {
        self.registry.register(adapter).await;
    }

    // ─── 消息发送 ───────────────────────────────────

    pub async fn send_text(
        &self,
        target: &MessageTarget,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let adapter = self
            .registry
            .get(&target.platform)
            .ok_or_else(|| AppError::NotFound(format!("未注册的 IM 平台: {}", target.platform)))?;
        adapter.send_text(target, text).await
    }

    pub async fn send_markdown(
        &self,
        target: &MessageTarget,
        title: &str,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let adapter = self
            .registry
            .get(&target.platform)
            .ok_or_else(|| AppError::NotFound(format!("未注册的 IM 平台: {}", target.platform)))?;
        adapter.send_markdown(target, title, text).await
    }

    pub async fn reply(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let adapter = self
            .registry
            .get(&original.platform)
            .ok_or_else(|| AppError::NotFound(format!("未注册的 IM 平台: {}", original.platform)))?;
        adapter.reply(original, text).await
    }

    // ─── 查询 ───────────────────────────────────────

    pub fn is_connected(&self, platform: &PlatformType) -> bool {
        self.registry.is_connected(platform)
    }

    pub fn platforms(&self) -> Vec<PlatformType> {
        self.registry.platforms()
    }

    pub fn adapter_count(&self) -> usize {
        self.registry.len()
    }

    // ─── 内部：入站消息处理循环 ──────────────────────

    async fn process_incoming_loop(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<IncomingMessage>) {
        tracing::info!("IMGateway 入站消息处理器已启动");

        // 初始化会话管理器
        let sessions_dir = crate::config::get_sessions_dir();
        let session_store = crate::storage::SessionStore::new(&sessions_dir);
        let default_model = {
            let state = crate::APP_STATE.read().await;
            state.models_config.default_model.clone()
        };
        let session_mgr = IMSessionManager::new(session_store, default_model);

        while let Some(msg) = rx.recv().await {
            // 群聊：检查是否需要响应
            if !im_session::should_respond_in_group(&msg) {
                tracing::debug!("群聊消息未提及机器人，跳过: {}", msg.id);
                continue;
            }

            let sender = msg.sender_name.as_deref().unwrap_or("?");
            tracing::info!(
                "IM → Agent: [{}] {}: {}",
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
        let source = im_session::session_source_from_incoming(&msg);

        // 1. 获取或创建 Agent 会话
        let session = session_mgr.get_or_create(&source, &msg).await?;

        // 2. 格式化用户消息（注入平台上下文）
        let user_text = im_session::format_im_message(&msg);

        // 3. 获取 LLM 客户端的配置
        let (provider, config, tool_registry, skills) = {
            let state = crate::APP_STATE.read().await;
            let model = &session.model;
            let provider = state
                .models_config
                .find_provider_by_model(model)
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
        let llm_client = LlmClient::new(provider, config.llm_timeout);
        let mut runtime = AgentRuntime::new(
            session,
            llm_client,
            tool_registry,
            &config,
            skills,
        );

        // 5. 执行 Agent（非流式，无取消）
        let result = runtime.run_turn(&user_text, None, None).await?;

        let reply_content = result.content.trim().to_string();

        // 6. 持久化会话消息
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
                });
        }

        // 7. 发送回复到 IM 平台
        if reply_content.is_empty() {
            tracing::warn!("Agent 返回空回复 (session={})", result.session_id);
            self.reply(&msg, "抱歉，我没有生成有效的回复，请重试。")
                .await
                .ok();
        } else {
            tracing::info!(
                "Agent → IM: {}，长度={}字符",
                &reply_content[..reply_content.len().min(50)],
                reply_content.len()
            );
            self.reply(&msg, &reply_content).await?;
        }

        Ok(())
    }
}
