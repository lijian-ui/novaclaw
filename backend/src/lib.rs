pub mod config;
pub mod error;
pub mod logging;
pub mod storage;
pub mod llm;
pub mod tools;
pub mod agent;
pub mod memory;
pub mod skills;
pub mod cron;
pub mod mcp;
pub mod server;
pub mod security;
pub mod soul;
pub mod dingtalk;
pub mod im;

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 全局应用状态
pub static APP_STATE: Lazy<Arc<RwLock<AppState>>> = Lazy::new(|| {
    Arc::new(RwLock::new(AppState::new()))
});

/// IM 网关全局实例（从 config/im.json 自动初始化）
pub static IM_GATEWAY: Lazy<Arc<RwLock<Option<Arc<im::IMGateway>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// 全局应用状态
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: config::AppConfig,
    pub models_config: config::ModelsConfig,
    pub tool_registry: tools::registry::ToolRegistry,
    pub session_store: storage::SessionStore,
    pub memory_store: memory::store::MemoryStore,
    pub skills_loader: skills::loader::SkillsLoader,
    pub soul_manager: crate::soul::SoulManager,
    pub approval_manager: tools::approval::ApprovalManager,
    /// 正在运行的流式会话取消标志表
    /// key: session_id, value: 取消标志
    pub cancel_map: HashMap<String, Arc<AtomicBool>>,
}

impl AppState {
    pub fn new() -> Self {
        config::ensure_directories_exists();
        let config = config::AppConfig::load();
        let models_config = config::ModelsConfig::load();
        Self {
            tool_registry: tools::registry::ToolRegistry::default(),
            session_store: storage::SessionStore::new(&config.sessions_dir()),
            memory_store: memory::store::MemoryStore::new(&config.memories_dir()),
            skills_loader: skills::loader::SkillsLoader::new(&config.skills_dir()),
            soul_manager: crate::soul::SoulManager::new(),
            approval_manager: tools::approval::ApprovalManager::new(),
            config,
            models_config,
            cancel_map: HashMap::new(),
        }
    }
}

/// 初始化应用状态并注册内置工具
pub async fn initialize() {
    // 确保默认 Soul 文件存在
    {
        let soul_loader = crate::soul::SoulLoader::new();
        if let Err(e) = soul_loader.ensure_default_soul() {
            tracing::warn!("Failed to ensure default soul: {:?}", e);
        }
    }

    // 先读取需要的配置值，然后释放写锁，再注册工具（避免 register_all 内部死锁）
    let (tinyfish_api_key, tavily_api_key, mut tool_registry, memory_store, skills_loader, session_store) = {
        let state = APP_STATE.read().await;
        (
            state.config.tinyfish_api_key.clone(),
            state.config.tavily_api_key.clone(),
            state.tool_registry.clone(),
            state.memory_store.clone(),
            state.skills_loader.clone(),
            state.session_store.clone(),
        )
    };

    // 在锁外注册工具，彻底避免死锁
    tools::builtin::register_all(
        &mut tool_registry,
        tinyfish_api_key,
        tavily_api_key,
        memory_store,
        skills_loader,
        session_store,
    );

    // 注册 MCP 已发现的工具（从持久化的 mcp.json 加载）
    crate::mcp::register_tools(&tool_registry).await;

    // 把注册好的 registry 写回 state
    {
        let mut state = APP_STATE.write().await;
        state.tool_registry = tool_registry;
        tracing::info!(
            "NovaClaw backend initialized\n  - 配置目录: {:?}\n  - 工作目录: {:?}\n  - 技能目录: {:?}\n  - 记忆目录: {:?}\n  - 会话目录: {:?}\n  - Soul 目录: {:?}\n  - 项目配置: {:?}\n  - 模型配置: {:?}",
            config::get_config_dir(),
            state.config.workspace_dir(),
            state.config.skills_dir(),
            state.config.memories_dir(),
            state.config.sessions_dir(),
            soul::SoulPaths::default().agent_default_dir,
            config::AppConfig::config_path(),
            config::ModelsConfig::models_path()
        );
    }

    // 启动后台定时清理过期确认请求（每 60 秒一次，超时 5 分钟）
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            crate::APP_STATE.read().await.approval_manager.cleanup_expired().await;
        }
    });

    // ─── 初始化 IM Gateway ─────────────────────────────
    let gateway = im::IMGateway::new();
    let im_config = im::config::load();

    for channel in &im_config.channels {
        if !channel.enabled {
            tracing::info!("IM 渠道已禁用，跳过: {}", channel.id);
            continue;
        }

        match channel.channel_type.as_str() {
            "dingtalk" => {
                if channel.use_stream_mode() {
                    tracing::info!("正在连接钉钉 Stream 模式...");
                    let cid = match channel.config.client_id.as_ref() {
                        Some(c) => c,
                        None => { tracing::warn!("钉钉渠道 '{}' 缺少 client_id，跳过", channel.name); continue; }
                    };
                    let cs = match channel.config.client_secret.as_ref() {
                        Some(c) => c,
                        None => { tracing::warn!("钉钉渠道 '{}' 缺少 client_secret，跳过", channel.name); continue; }
                    };
                    let dt_client = Arc::new(dingtalk::DingTalkClient::new(cid.clone(), cs.clone()).await);

                    let dt_adapter = Arc::new(dingtalk::adapter::DingTalkAdapter::new(dt_client.clone()));

                    // 注册回调处理器：将钉钉入站消息转发到 IMGateway
                    {
                        let incoming_tx = gateway.incoming_tx.clone();
                        dt_client
                            .register_handler(
                                crate::im::handler::IMGatewayCallbackHandler::new(incoming_tx),
                            )
                            .await;
                    }

                    gateway.register(dt_adapter).await;
                    tracing::info!("钉钉 Stream 模式已注册到 IMGateway");
                } else if channel.use_webhook_mode() {
                    tracing::info!("钉钉 Webhook 模式已配置 (webhook={})", 
                        channel.config.webhook.as_ref().map(|s| s.chars().take(40).collect::<String>()).unwrap_or("?".to_string()));
                    // Webhook 模式不需要注册适配器，由 HTTP 调用直接发送
                } else {
                    tracing::warn!("钉钉渠道 '{}' 没有有效的配置（需要 webhook 或 client_id+client_secret）", channel.name);
                }
            }
            _ => {
                tracing::warn!("不支持的 IM 渠道类型: {} (id={})", channel.channel_type, channel.id);
            }
        }
    }

    // 保存到全局
    {
        let mut g = IM_GATEWAY.write().await;
        *g = Some(gateway);
    }

    tracing::info!("IMGateway 初始化完成 ({} 个渠道配置)", im_config.channels.len());

    // MCP 连接改为惰性初始化：首次工具调用时自动连接，启动时不阻塞
}

/// 启动 Axum HTTP/WebSocket 服务器（供桌面版调用）
pub async fn start_server() {
    initialize().await;
    server::start().await;
}
