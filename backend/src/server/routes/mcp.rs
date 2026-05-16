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

async fn list_servers() -> Json<serde_json::Value> {
    tracing::info!("{}查询 MCP 服务器列表", LOG_PREFIX);
    let store = crate::mcp::get_store();
    let guard = store.lock().await;
    let servers = guard.list();
    tracing::debug!("{}MCP 服务器列表: {} 个", LOG_PREFIX, servers.len());
    Json(serde_json::json!(servers))
}

async fn create_server(Json(req): Json<CreateServerReq>) -> Json<serde_json::Value> {
    tracing::info!("{}创建 MCP 服务器: {}", LOG_PREFIX, req.name);
    
    let store = crate::mcp::get_store();
    let mut guard = store.lock().await;

    if guard.get(&req.name).is_some() {
        let err = format!("MCP 服务器 '{}' 已存在", req.name);
        tracing::warn!("{}{}", LOG_PREFIX, err);
        return Json(serde_json::json!({
            "success": false,
            "message": err
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
    };
    guard.add(server);
    tracing::info!("{}MCP 服务器创建成功: {}", LOG_PREFIX, req.name);
    Json(serde_json::json!({ "success": true }))
}

async fn delete_server(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}删除 MCP 服务器: {}", LOG_PREFIX, name);
    
    let store = crate::mcp::get_store();
    let mut guard = store.lock().await;
    let removed = guard.remove(&name);
    
    if removed {
        tracing::info!("{}MCP 服务器删除成功: {}", LOG_PREFIX, name);
    } else {
        tracing::warn!("{}MCP 服务器删除失败，未找到: {}", LOG_PREFIX, name);
    }
    
    Json(serde_json::json!({ "success": removed }))
}

async fn toggle_server(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}切换 MCP 服务器状态: {}", LOG_PREFIX, name);
    
    let store = crate::mcp::get_store();
    let mut guard = store.lock().await;
    let toggled = guard.toggle(&name);
    
    if toggled {
        let server = guard.get(&name);
        if let Some(s) = server {
            tracing::info!("{}MCP 服务器状态切换成功: {} -> {}", LOG_PREFIX, name, s.enabled);
        }
    } else {
        tracing::warn!("{}MCP 服务器状态切换失败，未找到: {}", LOG_PREFIX, name);
    }
    
    Json(serde_json::json!({ "success": toggled }))
}

async fn discover_tools(Path(name): Path<String>) -> Json<serde_json::Value> {
    tracing::info!("{}发现 MCP 服务器工具: {}", LOG_PREFIX, name);
    
    let server = {
        let store = crate::mcp::get_store();
        let guard = store.lock().await;
        guard.get(&name).cloned()
    };

    let server = match server {
        Some(s) => s,
        None => {
            let err = format!("MCP 服务器 '{}' 未找到", name);
            tracing::warn!("{}{}", LOG_PREFIX, err);
            return Json(serde_json::json!({
                "success": false,
                "message": err
            }));
        }
    };

    match crate::mcp::discover_tools(&server).await {
        Ok(tools) => {
            let store = crate::mcp::get_store();
            let mut guard = store.lock().await;
            guard.update_tools(&name, tools);
            tracing::info!("{}MCP 服务器工具发现成功: {}", LOG_PREFIX, name);
            Json(serde_json::json!({ "success": true }))
        }
        Err(e) => {
            tracing::error!("{}MCP 服务器工具发现失败: {} - {}", LOG_PREFIX, name, e);
            Json(serde_json::json!({ "success": false, "message": e }))
        }
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/mcp", get(list_servers))
        .route("/mcp", post(create_server))
        .route("/mcp/{name}", delete(delete_server))
        .route("/mcp/{name}/toggle", post(toggle_server))
        .route("/mcp/{name}/discover", post(discover_tools))
}
