use axum::{extract::Path, routing::{get, put, post, delete}, Json, Router};
use crate::APP_STATE;
use crate::config::AppConfig;
use crate::soul::{AgentConfig, SoulPaths};

/// 获取当前配置（从内存读取，不重新加载磁盘文件）
async fn get_config() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let config = state.config.clone();
    drop(state);

    Json(serde_json::json!({
        "success": true,
        "data": {
            "port": config.port,
            "host": config.host,
            "max_iterations": config.max_iterations,
            "compact_threshold": config.compact_threshold,
            "compact_keep": config.compact_keep,
            "temperature": config.temperature,
            "deny_patterns": config.deny_patterns,
            "shell_allowlist": config.shell_allowlist,
            "approval_mode": config.approval_mode,
            "memories_dir": config.memories_dir().to_string_lossy(),
        },
    }))
}

/// 更新配置
async fn update_config(Json(config): Json<AppConfig>) -> Json<serde_json::Value> {
    let mut state = APP_STATE.write().await;

    if !config.deny_patterns.is_empty() {
        if let Ok(mut cached) = crate::tools::execute::DENY_PATTERNS.write() {
            *cached = config.deny_patterns.clone();
        }
    }
    // 同步审批模式到运行时缓存
    if let Ok(mut cached) = crate::tools::execute::APPROVAL_MODE.write() {
        *cached = config.approval_mode.clone();
    }
    state.config = config;

    match state.config.save() {
        Ok(_) => {
            tracing::info!("项目配置已保存");
            Json(serde_json::json!({"success": true}))
        }
        Err(e) => {
            tracing::error!("保存配置失败: {}", e);
            Json(serde_json::json!({"success": false, "message": format!("保存配置失败: {}", e)}))
        }
    }
}

/// 前端选择默认智能体时记录日志
async fn log_default_agent() -> Json<serde_json::Value> {
    tracing::info!("[Agent] 前端选择智能体: 默认智能体");
    Json(serde_json::json!({"success": true}))
}

/// 前端选择智能体时调用，记录日志
async fn log_agent_selection(Path(agent_id): Path<String>) -> Json<serde_json::Value> {
    if agent_id == "default" {
        tracing::info!("[Agent] 前端选择智能体: id=default (默认智能体)");
        return Json(serde_json::json!({"success": true}));
    }
    let paths = SoulPaths::default();
    if let Ok(config) = AgentConfig::load(&paths, &agent_id) {
        tracing::info!("[Agent] 前端选择智能体: id={}, name={}", config.id, config.name);
    } else {
        tracing::warn!("[Agent] 前端选择智能体 '{}' 未找到", agent_id);
    }
    Json(serde_json::json!({"success": true}))
}

/// 列出所有智能体（包含 default）
async fn list_agents() -> Json<serde_json::Value> {
    let paths = SoulPaths::default();
    let mut agent_names = AgentConfig::list_all(&paths);

    // 始终保证 default 智能体在列表中
    if !agent_names.iter().any(|n| n == "default") {
        agent_names.push("default".to_string());
    }

    let mut agents = Vec::new();

    for name in agent_names {
        match AgentConfig::load(&paths, &name) {
            Ok(config) => {
                let has_soul = std::path::Path::new(&paths.soul_path(&name)).exists();
                agents.push(serde_json::json!({
                    "id": config.id,
                    "name": config.name,
                    "description": config.description,
                    "model": config.model,
                    "enabled_tools": config.enabled_tools,
                    "max_iterations": config.max_iterations,
                    "temperature": config.temperature,
                    "compact_threshold": config.compact_threshold,
                    "compact_keep": config.compact_keep,
                    "has_soul": has_soul,
                }));
            }
            Err(_) => {
                let has_soul = std::path::Path::new(&paths.soul_path(&name)).exists();
                agents.push(serde_json::json!({
                    "id": name,
                    "name": if name == "default" { "默认智能体" } else { &name },
                    "description": if name == "default" { "系统默认智能体，使用全局配置" } else { "" },
                    "model": null,
                    "enabled_tools": [],
                    "max_iterations": 0,
                    "temperature": null,
                    "compact_threshold": null,
                    "compact_keep": null,
                    "has_soul": has_soul,
                }));
            }
        }
    }

    Json(serde_json::json!({"success": true, "data": agents}))
}

/// 创建或更新智能体
async fn upsert_agent(Path(agent_id): Path<String>, Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let paths = SoulPaths::default();

    let config = AgentConfig {
        id: agent_id.clone(),
        name: body["name"].as_str().unwrap_or(&agent_id).to_string(),
        description: body["description"].as_str().unwrap_or("").to_string(),
        model: body.get("model").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()),
        enabled_tools: body.get("enabled_tools")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default(),
        max_iterations: body.get("max_iterations").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        compact_threshold: body.get("compact_threshold").and_then(|v| v.as_u64()).map(|v| v as usize),
        compact_keep: body.get("compact_keep").and_then(|v| v.as_u64()).map(|v| v as usize),
    };

    match config.save(&paths) {
        Ok(_) => {
            // 如果有 system_prompt，写入 SOUL.md
            if let Some(prompt) = body.get("system_prompt").and_then(|v| v.as_str()) {
                if !prompt.is_empty() {
                    let _ = AgentConfig::save_soul_content(&paths, &agent_id, prompt);
                }
            }
            tracing::info!("[Agent] 保存智能体: id={}, name={}", config.id, config.name);
            Json(serde_json::json!({"success": true}))
        }
        Err(e) => Json(serde_json::json!({"success": false, "message": e}))
    }
}

/// 获取智能体的 SOUL.md 内容
async fn get_agent_soul(Path(agent_id): Path<String>) -> Json<serde_json::Value> {
    let paths = SoulPaths::default();
    match AgentConfig::get_soul_content(&paths, &agent_id) {
        Ok(content) => Json(serde_json::json!({"success": true, "data": content})),
        Err(e) => Json(serde_json::json!({"success": false, "message": e})),
    }
}

/// 删除智能体（default 不可删除）
async fn delete_agent(Path(agent_id): Path<String>) -> Json<serde_json::Value> {
    let paths = SoulPaths::default();
    match AgentConfig::remove(&paths, &agent_id) {
        Ok(_) => {
            tracing::info!("[Agent] 删除智能体: id={}", agent_id);
            Json(serde_json::json!({"success": true}))
        }
        Err(e) => Json(serde_json::json!({"success": false, "message": e}))
    }
}

/// 获取 shell allowlist
async fn get_shell_allowlist() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let list = state.config.shell_allowlist.clone();
    Json(serde_json::json!({"success": true, "data": list}))
}

/// 添加 shell allowlist 条目
#[derive(serde::Deserialize)]
struct AddAllowlistReq {
    prefix: String,
}
async fn add_shell_allowlist(Json(req): Json<AddAllowlistReq>) -> Json<serde_json::Value> {
    let mut state = APP_STATE.write().await;
    let prefix = req.prefix.trim().to_lowercase();
    if prefix.is_empty() {
        return Json(serde_json::json!({"success": false, "message": "前缀不能为空"}));
    }
    // 去重添加
    if !state.config.shell_allowlist.contains(&prefix) {
        state.config.shell_allowlist.push(prefix.clone());
        // 同步到运行时缓存
        if let Ok(mut cached) = crate::tools::execute::ALLOW_PATTERNS.write() {
            *cached = state.config.shell_allowlist.clone();
        }
        // 保存到文件
        if let Err(e) = state.config.save() {
            tracing::error!("保存 allowlist 失败: {}", e);
        }
    }
    tracing::info!("[Allowlist] 添加命令前缀: {}", prefix);
    Json(serde_json::json!({"success": true}))
}

/// 删除 shell allowlist 条目
#[derive(serde::Deserialize)]
struct RemoveAllowlistReq {
    prefix: String,
}
async fn remove_shell_allowlist(Json(req): Json<RemoveAllowlistReq>) -> Json<serde_json::Value> {
    let mut state = APP_STATE.write().await;
    let prefix = req.prefix.trim().to_lowercase();
    state.config.shell_allowlist.retain(|p| p != &prefix);
    // 同步到运行时缓存
    if let Ok(mut cached) = crate::tools::execute::ALLOW_PATTERNS.write() {
        *cached = state.config.shell_allowlist.clone();
    }
    if let Err(e) = state.config.save() {
        tracing::error!("保存 allowlist 失败: {}", e);
    }
    tracing::info!("[Allowlist] 移除命令前缀: {}", prefix);
    Json(serde_json::json!({"success": true}))
}

pub fn routes() -> Router {
    Router::new()
        .route("/config", get(get_config))
        .route("/config", put(update_config))
        .route("/config/shell_allowlist", get(get_shell_allowlist))
        .route("/config/shell_allowlist", post(add_shell_allowlist))
        .route("/config/shell_allowlist", delete(remove_shell_allowlist))
        .route("/set-agent", get(log_default_agent))
        .route("/set-agent/:agent_id", get(log_agent_selection))
        .route("/agents", get(list_agents))
        .route("/agents/:agent_id", put(upsert_agent))
        .route("/agents/:agent_id/soul", get(get_agent_soul))
        .route("/agents/:agent_id", delete(delete_agent))
}
