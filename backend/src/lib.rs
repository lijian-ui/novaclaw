pub mod config;
pub mod error;
pub mod logging;
pub mod storage;
pub mod llm;
pub mod tools;
pub mod agent;
pub mod memory;
pub mod skills;
pub mod server;

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 全局应用状态
pub static APP_STATE: Lazy<Arc<RwLock<AppState>>> = Lazy::new(|| {
    Arc::new(RwLock::new(AppState::new()))
});

/// 全局应用状态
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: config::AppConfig,
    pub models_config: config::ModelsConfig,
    pub tool_registry: tools::registry::ToolRegistry,
    pub session_store: storage::SessionStore,
    pub memory_store: memory::store::MemoryStore,
    pub skills_loader: skills::loader::SkillsLoader,
    /// 正在运行的流式会话取消标志表
    /// key: session_id, value: 取消标志
    /// 用于 SSE cancel 端点中断正在进行的流式生成
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
            config,
            models_config,
            cancel_map: HashMap::new(),
        }
    }
}

/// 初始化应用状态并注册内置工具
pub async fn initialize() {
    // 先读取需要的配置值，然后释放写锁，再注册工具（避免 register_all 内部死锁）
    let (tinyfish_api_key, tavily_api_key, mut tool_registry, memory_store, skills_loader) = {
        let state = APP_STATE.read().await;
        (
            state.config.tinyfish_api_key.clone(),
            state.config.tavily_api_key.clone(),
            state.tool_registry.clone(),
            state.memory_store.clone(),
            state.skills_loader.clone(),
        )
    };

    // 在锁外注册工具，彻底避免死锁
    tools::builtin::register_all(
        &mut tool_registry,
        tinyfish_api_key,
        tavily_api_key,
        memory_store,
        skills_loader,
    );

    // 把注册好的 registry 写回 state
    {
        let mut state = APP_STATE.write().await;
        state.tool_registry = tool_registry;
        tracing::info!(
            "NovaClaw backend initialized\n  - 配置目录: {:?}\n  - 工作目录: {:?}\n  - 技能目录: {:?}\n  - 记忆目录: {:?}\n  - 会话目录: {:?}\n  - 项目配置: {:?}\n  - 模型配置: {:?}",
            config::get_config_dir(),
            state.config.workspace_dir(),
            state.config.skills_dir(),
            state.config.memories_dir(),
            state.config.sessions_dir(),
            config::AppConfig::config_path(),
            config::ModelsConfig::models_path()
        );
    }
}

/// 启动 Axum HTTP/WebSocket 服务器（供桌面版调用）
pub async fn start_server() {
    initialize().await;
    server::start().await;
}
