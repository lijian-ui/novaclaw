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
pub mod bg_task;
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

    // 初始化默认子智能体（仅在首次启动时创建）
    {
        let paths = crate::soul::SoulPaths::default();
        let default_agents: Vec<(&str, &str, &str, Vec<&str>)> = vec![
            ("code-reviewer", "代码审查员", "审查代码质量、发现 Bug 和安全问题",
             vec!["read_file", "search", "glob", "list_dir"]),
            ("data-analyst", "数据分析师", "处理数据、统计分析、生成报告",
             vec!["read_file", "write_file", "execute_command", "glob"]),
            ("web-researcher", "网络研究员", "搜索网络信息、整理资料",
             vec!["web_search", "web_fetch", "read_file", "search"]),
        ];
        for (id, name, desc, tools) in default_agents {
            if !std::path::Path::new(&paths.agent_json_path(id)).exists() {
                let config = crate::soul::AgentConfig {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: desc.to_string(),
                    model: None,
                    enabled_tools: tools.iter().map(|t| t.to_string()).collect(),
                    max_iterations: 0,
                    temperature: None,
                    compact_threshold: None,
                    compact_keep: None,
                };
                if let Err(e) = config.save(&paths) {
                    tracing::warn!("创建默认智能体 '{}' 失败: {}", id, e);
                }
            }
            // 如果 SOUL.md 不存在也创建
            if !std::path::Path::new(&paths.soul_path(id)).exists() {
                let soul_content = match id {
                    "code-reviewer" => "你是一个严谨的代码审查员。你的职责是：\n1. 仔细阅读代码，找出逻辑错误、性能问题、安全隐患\n2. 检查代码风格是否符合最佳实践\n3. 给出具体的改进建议\n4. 如果代码没有问题，明确说明「代码审查通过」\n\n请专注审查，不要执行修改操作。",
                    "data-analyst" => "你是一个专业的数据分析师。你的职责是：\n1. 理解数据分析需求\n2. 使用 Python 或其他工具处理数据\n3. 分析结果并用清晰的语言解释\n4. 必要时生成可视化图表\n\n始终展示你的分析过程和结论。",
                    "web-researcher" => "你是一个高效的网络研究员。你的职责是：\n1. 通过网络搜索查找相关信息\n2. 从多个来源交叉验证信息准确性\n3. 整理和归纳搜索结果为结构化报告\n4. 注明信息来源\n\n确保信息准确可靠，不确定时明确说明。",
                    _ => "",
                };
                if !soul_content.is_empty() {
                    let _ = crate::soul::AgentConfig::save_soul_content(&paths, id, soul_content);
                }
            }
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

    // ─── 初始化 IM Gateway（支持多账号） ────────────────
    let gateway = im::IMGateway::new();
    let im_config = im::config::load();
    let mut total_accounts = 0usize;

    for channel in &im_config.channels {
        if !channel.enabled {
            tracing::info!("IM 渠道已禁用，跳过: {}", channel.id);
            continue;
        }

        match channel.channel_type.as_str() {
            "dingtalk" => {
                let account_ids = channel.enabled_account_ids();

                if account_ids.is_empty() {
                    if channel.use_webhook_mode() {
                        tracing::info!("钉钉 Webhook 模式已配置 (id={})", channel.id);
                    } else {
                        tracing::warn!("钉钉渠道 '{}' 没有有效的账号配置", channel.name);
                    }
                    continue;
                }

                for account_id in &account_ids {
                    let account_cfg = match channel.get_account(account_id) {
                        Some(c) => c,
                        None => { tracing::warn!("账号 '{}' 配置获取失败，跳过", account_id); continue; }
                    };

                    if !account_cfg.enabled {
                        tracing::info!("钉钉账号已禁用，跳过: {}", account_id);
                        continue;
                    }

                    tracing::info!(
                        "正在连接钉钉账号: {} (name={:?})",
                        account_id, account_cfg.name
                    );

                    let dt_client = Arc::new(
                        dingtalk::DingTalkClient::new(
                            account_id.clone(),
                            account_cfg.name.clone(),
                            account_cfg.credentials.client_id.clone(),
                            account_cfg.credentials.client_secret.clone(),
                        )
                        .await,
                    );

                    let dt_adapter = Arc::new(
                        dingtalk::adapter::DingTalkAdapter::new(dt_client.clone())
                    );

                    // 注册回调处理器：将钉钉入站消息转发到 IMGateway
                    {
                        let incoming_tx = gateway.incoming_tx.clone();
                        let acc_id = account_id.clone();
                        dt_client
                            .register_handler(
                                crate::im::handler::IMGatewayCallbackHandler::new(
                                    incoming_tx,
                                    acc_id,
                                ),
                            )
                            .await;
                    }

                    gateway.register(im::registry::AccountInfo {
                        account_id: account_id.clone(),
                        platform: im::types::PlatformType::DingTalk,
                        adapter: dt_adapter.clone(),
                        enabled: true,
                        name: account_cfg.name.clone(),
                    }).await;

                    total_accounts += 1;
                    tracing::info!("钉钉账号已注册: {} (name={:?})", account_id, account_cfg.name);
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

    tracing::info!("IMGateway 初始化完成 ({} 个账号)", total_accounts);

    // MCP 连接改为惰性初始化：首次工具调用时自动连接，启动时不阻塞
}

/// 启动 Axum HTTP/WebSocket 服务器（供桌面版调用）
/// 注意：此函数假设 `initialize()` 已经在调用前执行过
pub async fn start_server() {
    server::start().await;
}
