use serde_json::Value;
use std::path::{Path, PathBuf};

use super::registry::ToolRegistry;

pub mod file_ops;
pub mod web_search;
pub mod web_fetch;
pub mod apply_patch;
pub mod plan_tools;
pub mod todo_tools;
pub mod execute;
pub mod cron;
pub mod delegate;
pub mod memory_tools;
pub mod im_push;
pub mod agent_manage;

/// 解析文件路径
/// 如果是相对路径，优先使用会话的工作目录；没有则使用全局默认 workspace
/// 如果是绝对路径，直接返回
pub(crate) fn resolve_path(path_str: &str, args: &Value) -> PathBuf {
    let path = Path::new(path_str);

    if path.is_absolute() {
        return path.to_path_buf();
    }

    // 优先使用注入的 session workspace
    if let Some(ws) = args.get("_workspace").and_then(|v| v.as_str()) {
        return PathBuf::from(ws).join(path);
    }

    // 兜底：使用全局默认 workspace
    crate::config::get_workspace_dir().join(path)
}

/// 简单 glob 匹配（检查 name 是否包含 pattern 去掉 * 后的部分）
pub(crate) fn glob_match(name: &str, pattern: &str) -> bool {
    let pattern = pattern.replace("*", "");
    name.contains(&pattern)
}

/// 注册所有内置工具
/// 所有需要访问全局状态的数据（api key、memory_store、skills_loader）均从外部传入，
/// 避免在 handler 内部调用 APP_STATE.blocking_read()（在 tokio worker 上会 panic）
pub fn register_all(
    registry: &mut ToolRegistry,
    tinyfish_api_key: Option<String>,
    tavily_api_key: Option<String>,
    memory_store: crate::memory::store::MemoryStore,
    skills_loader: crate::skills::loader::SkillsLoader,
    session_store: crate::storage::SessionStore,
) {
    let rt = tokio::runtime::Handle::current();
    let registry_clone = registry.clone();
    let memory_store = std::sync::Arc::new(memory_store);
    let skills_loader = std::sync::Arc::new(skills_loader);
    let session_store = std::sync::Arc::new(session_store);

    std::thread::spawn(move || {
        rt.block_on(async move {
            file_ops::register(&registry_clone).await;
            memory_tools::register(&registry_clone, &memory_store, &session_store, &skills_loader).await;
            todo_tools::register(&registry_clone).await;
            plan_tools::register(&registry_clone).await;
            web_search::register(&registry_clone, &tinyfish_api_key, &tavily_api_key).await;
            web_fetch::register(&registry_clone).await;
            apply_patch::register(&registry_clone).await;
            execute::register(&registry_clone).await;
            cron::register(&registry_clone).await;
            delegate::register(&registry_clone).await;
            im_push::register(&registry_clone).await;
            agent_manage::register(&registry_clone).await;
            tracing::info!("内置工具注册完成");
        });
    })
    .join()
    .ok();
}
