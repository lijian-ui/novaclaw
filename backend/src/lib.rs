pub mod config;
pub mod error;
pub mod storage;
pub mod llm;
pub mod tools;
pub mod agent;
pub mod memory;
pub mod skills;
pub mod server;

use once_cell::sync::Lazy;
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
        }
    }
}

/// 初始化应用状态并注册内置工具
pub async fn initialize() {
    let mut state = APP_STATE.write().await;
    tools::builtin::register_all(&mut state.tool_registry);
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

/// 启动 Axum HTTP/WebSocket 服务器（供桌面版调用）
pub async fn start_server() {
    initialize().await;
    server::start().await;
}
