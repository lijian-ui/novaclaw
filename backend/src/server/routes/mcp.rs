//! MCP 服务器管理 API
//!
//! - 创建服务时自动连接 + 发现工具（无需重启）
//! - 列表返回实时连接状态
//! - 支持手动连接/断开

use axum::{
    extract::Path,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;

const LOG_PREFIX: &str = "[MCP] ";

#[derive(Deserialize)]
struct CreateServerReq {
    name: String,
    #[serde(default)]
    transport_type: String,
    command: Option<String>,
    args: Option<Vec<String>>,
    url: Option<String>,
    headers: Option<std::collections::HashMap<String, String>>,
    description: Option<String>,
}

/// 列出所有 MCP 服务器（含运行时连接状态）
async fn list_servers() -> Json<serde_json::Value> {
    let servers = crate::mcp::get_all_servers_with_status().await;
    Json(serde_json::json!(servers))
}

/// 创建 MCP 服务器 → 保存配置 → 自动连接 → 发现工具
async fn create_server(Json(req): Json<CreateServerReq>) -> Json<serde_json::Value> {
    tracing::info!("{}创建 MCP 服务器: {}", LOG_PREFIX, req.name);

    let store = crate::mcp::get_store();
    let mut guard = store.lock().await;

    if guard.get(&req.name).is_some() {
        return Json(serde_json::json!({
            "success": false,
            "message": format!("MCP 服务器 '{}' 已存在", req.name)
        }));
    }

    let server = crate::mcp::McpServerConfig {
        name: req.name.clone(),
        transport_type: if req.transport_type.is_empty() { "stdio".to_string() } else { req.transport_type },
        command: req.command,
        args: req.args,
        url: req.url,
        headers: req.headers,
        description: req.description.unwrap_or_default(),
        enabled: true,
        tools: Vec::new(),
        status: crate::mcp::ConnectionStatus::Disconnected,
    };

    // 保存到配置文件
    guard.add(server);
    // 拿到一份副本（释放锁后再使用）
    let server_copy = guard.get(&req.name).cloned();
    drop(guard);

    let server_copy = match server_copy {
        Some(s) => s,
        None => return Json(serde_json::json!({ "success": false, "message": "保存后找不到服务器" })),
    };

    // 自动连接（stdio 模式）
    if server_copy.transport_type == "stdio" {
        tracing::info!("{}正在自动连接 MCP: {}", LOG_PREFIX, req.name);

        // 更新状态为 Connecting
        {
            let store_arc = crate::mcp::get_store();
            let mut store = store_arc.lock().await;
            if let Some(s) = store.get_mut(&req.name) {
                s.status = crate::mcp::ConnectionStatus::Connecting;
            }
        }

        match crate::mcp::connect_server(&server_copy).await {
            Ok(()) => {
                tracing::info!("{}MCP 连接成功，正在发现工具: {}", LOG_PREFIX, req.name);
                // 连接成功 → 自动发现工具
                match crate::mcp::discover_tools(&server_copy).await {
                    Ok(tools) => {
                        let store = crate::mcp::get_store();
                        let mut guard = store.lock().await;
                        guard.update_tools(&req.name, tools);
                        drop(guard);
                        // 注册到 ToolRegistry
                        let registry = crate::APP_STATE.read().await.tool_registry.clone();
                        crate::mcp::register_tools(&registry).await;
                        tracing::info!("{}MCP 工具发现完成: {}", LOG_PREFIX, req.name);
                        Json(serde_json::json!({
                            "success": true,
                            "connected": true,
                            "message": "MCP 服务器已创建并连接成功"
                        }))
                    }
                    Err(e) => {
                        tracing::warn!("{}MCP 连接成功但工具发现失败: {} - {}", LOG_PREFIX, req.name, e);
                        Json(serde_json::json!({
                            "success": true,
                            "connected": true,
                            "message": format!("MCP 服务器已创建并连接，但工具发现失败: {}", e)
                        }))
                    }
                }
            }
            Err(e) => {
                tracing::error!("{}MCP 连接失败: {} - {}", LOG_PREFIX, req.name, e);
                Json(serde_json::json!({
                    "success": true,
                    "connected": false,
                    "message": format!("MCP 服务器配置已保存，但连接失败: {}", e)
                }))
            }
        }
    } else {
        // SSE 模式不需要连接
        let registry = crate::APP_STATE.read().await.tool_registry.clone();
        crate::mcp::register_tools(&registry).await;
        Json(serde_json::json!({
            "success": true,
            "connected": true,
            "message": "MCP 服务器已创建"
        }))
    }
}

/// 删除 MCP 服务器（先断开连接）
async fn delete_server(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}删除 MCP 服务器: {}", LOG_PREFIX, name);

    // 先断开连接
    crate::mcp::disconnect_server(&name).await;

    let store = crate::mcp::get_store();
    let mut guard = store.lock().await;
    let removed = guard.remove(&name);
    drop(guard);

    // 从 ToolRegistry 中移除该服务器的工具
    let registry = crate::APP_STATE.read().await.tool_registry.clone();
    crate::mcp::register_tools(&registry).await;

    Json(serde_json::json!({ "success": removed }))
}

/// 切换启用/禁用
async fn toggle_server(Path(name): Path<String>) -> Json<serde_json::Value> {
    let server = {
        let store = crate::mcp::get_store();
        let guard = store.lock().await;
        guard.get(&name).cloned()
    };

    let server = match server {
        Some(s) => s,
        None => return Json(serde_json::json!({ "success": false })),
    };

    // 如果当前是启用状态要切换为禁用 → 断开连接
    if server.enabled {
        crate::mcp::disconnect_server(&name).await;
    }

    let store = crate::mcp::get_store();
    let mut guard = store.lock().await;
    let toggled = guard.toggle(&name);

    if toggled {
        let s = guard.get(&name);
        if let Some(s) = s {
            tracing::info!("{}MCP 服务器状态切换: {} -> enabled={}", LOG_PREFIX, name, s.enabled);
        }

        let was_enabled = s.map(|s| s.enabled).unwrap_or(false);
        drop(guard);

        if was_enabled {
            // 切换为启用 → 自动连接
            let store_arc = crate::mcp::get_store();
            let s = store_arc.lock().await;
            let server = s.get(&name).cloned();
            drop(s);
            if let Some(server) = server {
                let _ = crate::mcp::connect_server(&server).await;
            }
        }

        // 注册到 ToolRegistry（启用则注入工具，禁用则移除工具）
        let registry = crate::APP_STATE.read().await.tool_registry.clone();
        crate::mcp::register_tools(&registry).await;
    } else {
        drop(guard);
    }

    Json(serde_json::json!({ "success": toggled }))
}

/// 发现 MCP 服务器工具（自动连接如果未连接）
async fn discover_tools(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}发现 MCP 服务器工具: {}", LOG_PREFIX, name);

    let server = {
        let store = crate::mcp::get_store();
        let guard = store.lock().await;
        guard.get(&name).cloned()
    };

    let server = match server {
        Some(s) => s,
        None => return Json(serde_json::json!({
            "success": false, "message": format!("MCP 服务器 '{}' 未找到", name)
        })),
    };

    // stdio 模式：如果未连接，先自动连接
    if server.transport_type == "stdio" {
        let mgr = crate::mcp::get_connection_manager();
        let has = mgr.lock().await.has_stdio(&name);
        drop(mgr);
        if !has {
            tracing::info!("{}MCP 未连接，正在自动连接: {}", LOG_PREFIX, name);
            if let Err(e) = crate::mcp::connect_server(&server).await {
                return Json(serde_json::json!({
                    "success": false, "message": format!("连接失败: {}", e)
                }));
            }
        }
    }

    match crate::mcp::discover_tools(&server).await {
        Ok(tools) => {
            let store = crate::mcp::get_store();
            let mut guard = store.lock().await;
            guard.update_tools(&name, tools);
            drop(guard);
            // 注册到 ToolRegistry
            let registry = crate::APP_STATE.read().await.tool_registry.clone();
            crate::mcp::register_tools(&registry).await;
            tracing::info!("{}MCP 工具发现成功: {}", LOG_PREFIX, name);
            Json(serde_json::json!({ "success": true }))
        }
        Err(e) => {
            tracing::error!("{}MCP 工具发现失败: {} - {}", LOG_PREFIX, name, e);
            Json(serde_json::json!({ "success": false, "message": e }))
        }
    }
}

/// 手动连接 MCP 服务器
async fn connect_server(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}手动连接 MCP: {}", LOG_PREFIX, name);

    let server = {
        let store = crate::mcp::get_store();
        let guard = store.lock().await;
        guard.get(&name).cloned()
    };

    let server = match server {
        Some(s) => s,
        None => return Json(serde_json::json!({
            "success": false, "message": format!("MCP 服务器 '{}' 未找到", name)
        })),
    };

    match crate::mcp::connect_server(&server).await {
        Ok(()) => Json(serde_json::json!({ "success": true, "status": "connected" })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e })),
    }
}

/// 手动断开 MCP 服务器
async fn disconnect_server(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}手动断开 MCP: {}", LOG_PREFIX, name);
    crate::mcp::disconnect_server(&name).await;
    Json(serde_json::json!({ "success": true }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/mcp", get(list_servers))
        .route("/mcp", post(create_server))
        .route("/mcp/:name", delete(delete_server))
        .route("/mcp/:name/toggle", post(toggle_server))
        .route("/mcp/:name/discover", post(discover_tools))
        .route("/mcp/:name/connect", post(connect_server))
        .route("/mcp/:name/disconnect", post(disconnect_server))
}
