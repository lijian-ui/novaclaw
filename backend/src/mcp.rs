//! MCP (Model Context Protocol) 客户端
//!
//! ## 连接生命周期
//!
//! 1. 用户在前端保存 MCP 配置 → 立即初始化连接（无需重启）
//! 2. 前端实时显示每个服务的连接状态
//! 3. 项目关闭时统一清理

use rmcp::model::{CallToolRequestParams, RawContent};
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::RoleClient;
use rmcp::service::RunningService;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

const LOG_PREFIX: &str = "[MCP] ";

// ─── 连接状态 ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    #[serde(rename = "failed")]
    Failed(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::Disconnected
    }
}

// ─── 类型定义 ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default)]
    pub transport_type: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<McpToolInfo>,
    #[serde(default, skip_serializing)]
    pub status: ConnectionStatus,
}

fn default_enabled() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

// ─── 持久化存储 ──────────────────────────────────────────

pub struct McpStore {
    path: PathBuf,
    servers: Vec<McpServerConfig>,
}

impl McpStore {
    fn path() -> PathBuf {
        crate::config::get_config_dir().join("mcp.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let servers = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let result: Vec<McpServerConfig> = serde_json::from_str(&content).unwrap_or_default();
                    tracing::info!("{}加载 MCP 配置文件: {:?}, 共 {} 个服务器", LOG_PREFIX, path, result.len());
                    result
                }
                Err(e) => {
                    tracing::warn!("{}读取 MCP 配置文件失败: {:?}, 错误: {}", LOG_PREFIX, path, e);
                    Vec::new()
                }
            }
        } else {
            tracing::info!("{}MCP 配置文件不存在: {:?}", LOG_PREFIX, path);
            Vec::new()
        };
        Self { path, servers }
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        #[derive(Serialize)]
        struct ServerForSave {
            name: String,
            #[serde(rename = "transport_type", default)]
            transport_type: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            command: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            args: Option<Vec<String>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            url: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            headers: Option<HashMap<String, String>>,
            #[serde(default)]
            description: String,
            #[serde(default = "default_enabled")]
            enabled: bool,
        }
        let save_servers: Vec<ServerForSave> = self.servers.iter().map(|s| ServerForSave {
            name: s.name.clone(), transport_type: s.transport_type.clone(),
            command: s.command.clone(), args: s.args.clone(), url: s.url.clone(),
            headers: s.headers.clone(), description: s.description.clone(),
            enabled: s.enabled,
        }).collect();
        if let Ok(content) = serde_json::to_string_pretty(&save_servers) {
            if let Err(e) = std::fs::write(&self.path, content) {
                tracing::warn!("{}保存 MCP 配置文件失败: {:?}, 错误: {}", LOG_PREFIX, self.path, e);
            }
        }
    }

    pub fn list(&self) -> &[McpServerConfig] { &self.servers }
    pub fn get(&self, name: &str) -> Option<&McpServerConfig> { self.servers.iter().find(|s| s.name == name) }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut McpServerConfig> { self.servers.iter_mut().find(|s| s.name == name) }
    pub fn add(&mut self, server: McpServerConfig) { self.servers.push(server); self.save(); }
    pub fn remove(&mut self, name: &str) -> bool {
        let len = self.servers.len(); self.servers.retain(|s| s.name != name);
        let removed = self.servers.len() < len; if removed { self.save(); } removed
    }
    pub fn toggle(&mut self, name: &str) -> bool {
        if let Some(s) = self.get_mut(name) { s.enabled = !s.enabled; self.save(); true } else { false }
    }
    pub fn update_tools(&mut self, name: &str, tools: Vec<McpToolInfo>) -> bool {
        if let Some(s) = self.get_mut(name) { s.tools = tools; self.save(); true } else { false }
    }
}

static MCP_STORE: once_cell::sync::Lazy<Arc<Mutex<McpStore>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(McpStore::load())));

pub fn get_store() -> Arc<Mutex<McpStore>> { MCP_STORE.clone() }

// ─── 长驻连接管理器 ──────────────────────────────────────

/// stdio 连接 — 存储 RunningService，通过 Arc<Mutex> 提供 &mut self 能力
struct StdioConnection {
    /// Arc 允许多个引用共享；Mutex 允许 close() 时获得 &mut self
    running: Arc<Mutex<Option<RunningService<RoleClient, ()>>>>,
    #[allow(dead_code)]
    name: String,
}

pub struct McpConnectionManager {
    stdio: HashMap<String, StdioConnection>,
    http_client: reqwest::Client,
}

impl McpConnectionManager {
    fn new() -> Self {
        Self {
            stdio: HashMap::new(),
            http_client: reqwest::Client::builder()
                .pool_max_idle_per_host(10).timeout(std::time::Duration::from_secs(30))
                .build().expect("创建 HTTP 客户端失败"),
        }
    }

    pub async fn connect_stdio(&mut self, server: &McpServerConfig) -> Result<(), String> {
        if self.stdio.contains_key(&server.name) {
            return Ok(());
        }
        tracing::info!("{}启动 MCP 常驻进程: {}", LOG_PREFIX, server.name);
        let cmd = server.command.as_deref().ok_or("缺少启动命令")?;
        let args: Vec<&str> = server.args.as_ref().map(|a| a.iter().map(|s| s.as_str()).collect()).unwrap_or_default();
        let mut command = tokio::process::Command::new(cmd);
        command.args(&args);
        let transport = TokioChildProcess::new(command).map_err(|e| format!("创建 MCP 子进程失败: {}", e))?;
        let running = rmcp::service::serve_client((), transport).await.map_err(|e| format!("MCP 连接失败: {}", e))?;
        self.stdio.insert(server.name.clone(), StdioConnection {
            running: Arc::new(Mutex::new(Some(running))),
            name: server.name.clone(),
        });
        tracing::info!("{}MCP 常驻进程已启动: {}", LOG_PREFIX, server.name);
        Ok(())
    }

    pub async fn disconnect_stdio(&mut self, name: &str) {
        if let Some(conn) = self.stdio.remove(name) {
            if let Some(mut running) = conn.running.lock().await.take() {
                let _ = running.close().await;
            }
            tracing::info!("{}MCP 连接已断开: {}", LOG_PREFIX, name);
        }
    }

    pub fn has_stdio(&self, name: &str) -> bool { self.stdio.contains_key(name) }
    pub fn connected_stdio_names(&self) -> Vec<String> { self.stdio.keys().cloned().collect() }

    pub async fn shutdown_all(&mut self) {
        for (name, conn) in self.stdio.drain() {
            if let Some(mut running) = conn.running.lock().await.take() {
                let _ = running.close().await;
            }
            tracing::info!("{}MCP 连接已关闭: {}", LOG_PREFIX, name);
        }
    }
}

static MCP_CONNECTIONS: once_cell::sync::Lazy<Arc<Mutex<McpConnectionManager>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(McpConnectionManager::new())));

pub fn get_connection_manager() -> Arc<Mutex<McpConnectionManager>> { MCP_CONNECTIONS.clone() }

pub async fn get_all_servers_with_status() -> Vec<McpServerConfig> {
    let store = MCP_STORE.lock().await;
    let mgr = MCP_CONNECTIONS.lock().await;
    let connected = mgr.connected_stdio_names();
    let mut result: Vec<McpServerConfig> = store.list().to_vec();
    for server in &mut result {
        server.status = if connected.contains(&server.name) { ConnectionStatus::Connected }
            else { ConnectionStatus::Disconnected };
    }
    result
}

pub async fn connect_server(server: &McpServerConfig) -> Result<(), String> {
    {
        let mut store = MCP_STORE.lock().await;
        if let Some(s) = store.get_mut(&server.name) { s.status = ConnectionStatus::Connecting; }
    }
    let result = match server.transport_type.as_str() {
        "stdio" => { let mut mgr = MCP_CONNECTIONS.lock().await; mgr.connect_stdio(server).await }
        _ => Err(format!("传输类型 {} 暂不支持按需连接", server.transport_type)),
    };
    {
        let mut store = MCP_STORE.lock().await;
        if let Some(s) = store.get_mut(&server.name) {
            s.status = match &result { Ok(_) => ConnectionStatus::Connected, Err(e) => ConnectionStatus::Failed(e.clone()) };
        }
    }
    result
}

pub async fn disconnect_server(name: &str) {
    let mut mgr = MCP_CONNECTIONS.lock().await;
    mgr.disconnect_stdio(name).await;
    let mut store = MCP_STORE.lock().await;
    if let Some(s) = store.get_mut(name) { s.status = ConnectionStatus::Disconnected; }
}

pub async fn initialize_connections() {
    let store = MCP_STORE.lock().await;
    let enabled: Vec<_> = store.list().iter().filter(|s| s.enabled && s.transport_type == "stdio").cloned().collect();
    drop(store);
    if enabled.is_empty() { tracing::info!("{}没有需要初始化的 MCP 连接", LOG_PREFIX); return; }
    for server in &enabled {
        tracing::info!("{}正在初始化 MCP: {}", LOG_PREFIX, server.name);
        if let Err(e) = connect_server(server).await { tracing::error!("{}初始化 MCP 失败 ({}): {}", LOG_PREFIX, server.name, e); }
    }
}

pub async fn shutdown_all_connections() {
    let mut mgr = MCP_CONNECTIONS.lock().await;
    mgr.shutdown_all().await;
}

// ─── 工具发现 ────────────────────────────────────────────

pub async fn discover_tools(server: &McpServerConfig) -> Result<Vec<McpToolInfo>, String> {
    match server.transport_type.as_str() {
        "stdio" => {
            let tools = {
                let mgr = MCP_CONNECTIONS.lock().await;
                let conn = mgr.stdio.get(&server.name).ok_or_else(|| format!("MCP 连接未建立: {}", server.name))?;
                let running = conn.running.lock().await;
                let running = running.as_ref().ok_or_else(|| format!("MCP 连接已关闭: {}", server.name))?;
                running.peer().list_all_tools().await.map_err(|e| format!("获取工具列表失败: {}", e))?
            };
            Ok(tools.into_iter().map(|t| McpToolInfo {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                input_schema: serde_json::to_value(&*t.input_schema).unwrap_or_default(),
            }).collect())
        }
        "sse" => { let url = server.url.as_deref().ok_or("缺少 URL")?; sse_list_tools(url).await }
        t => Err(format!("不支持的传输类型: {}", t)),
    }
}

// ─── 工具调用 ────────────────────────────────────────────

pub async fn call_tool(server: &McpServerConfig, tool_name: &str, args: serde_json::Value) -> Result<String, String> {
    match server.transport_type.as_str() {
        "stdio" => {
            // 惰性连接：工具调用时自动连接（如尚未连接）
            {
                let mgr = MCP_CONNECTIONS.lock().await;
                if !mgr.has_stdio(&server.name) {
                    drop(mgr);
                    tracing::info!("{}惰性连接 MCP: {}", LOG_PREFIX, server.name);
                    connect_server(server).await?;
                }
            }
            let output = {
                let mgr = MCP_CONNECTIONS.lock().await;
                let conn = mgr.stdio.get(&server.name).ok_or_else(|| format!("MCP 连接未建立: {}", server.name))?;
                let running = conn.running.lock().await;
                let running = running.as_ref().ok_or_else(|| format!("MCP 连接已关闭: {}", server.name))?;
                let json_object: serde_json::Map<String, serde_json::Value> = match args {
                    serde_json::Value::Object(map) => map,
                    other => { let mut map = serde_json::Map::new(); map.insert("value".to_string(), other); map }
                };
                let result = running.peer().call_tool(
                    CallToolRequestParams::new(tool_name.to_string()).with_arguments(json_object),
                ).await.map_err(|e| format!("调用工具失败: {}", e))?;
                let mut output = String::new();
                for content in result.content {
                    match content.raw { RawContent::Text(t) => { output.push_str(&t.text); output.push('\n'); } _ => {} }
                }
                output.trim().to_string()
            };
            Ok(output)
        }
        "sse" => {
            let url = server.url.as_deref().ok_or("缺少 URL")?;
            let mgr = MCP_CONNECTIONS.lock().await;
            sse_call_tool_with_client(&mgr.http_client, url, tool_name, args).await
        }
        t => Err(format!("不支持的传输类型: {}", t)),
    }
}

// ─── SSE 传输 ───────────────────────────────────────────

async fn sse_request(client: &reqwest::Client, url: &str, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut req_body = json!({"jsonrpc": "2.0", "id": request_id, "method": method});
    if let Some(p) = params { req_body["params"] = p; }
    let resp = client.post(url).header("Content-Type", "application/json").json(&req_body).send().await
        .map_err(|e| format!("SSE POST 请求失败: {}", e))?;
    let body = resp.bytes().await.map_err(|e| format!("读取响应失败: {}", e))?;
    if body.is_empty() { return Err("SSE 返回空响应".to_string()); }
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) {
        if val.get("jsonrpc").is_some() {
            if let Some(err) = val.get("error") { return Err(format!("MCP 错误: {}", err.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误"))); }
            return Ok(val.get("result").cloned().unwrap_or(val));
        }
    }
    let text = String::from_utf8_lossy(&body);
    let mut data_parts: Vec<&str> = Vec::new(); let mut in_data = false;
    for line in text.lines() {
        if line.starts_with("data: ") { data_parts.push(line.trim_start_matches("data: ")); in_data = true; }
        else if in_data && !line.starts_with("event:") && !line.is_empty() { data_parts.push(line.trim()); }
        else if line.is_empty() && in_data {
            let data_str = data_parts.join(""); data_parts.clear(); in_data = false;
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data_str) {
                if let Some(err) = val.get("error") { return Err(format!("MCP 错误: {}", err.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误"))); }
                return Ok(val.get("result").cloned().unwrap_or(val));
            }
        }
    }
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) { return Ok(val); }
    Err(format!("无法解析响应: {}", text.chars().take(200).collect::<String>()))
}

async fn sse_list_tools(url: &str) -> Result<Vec<McpToolInfo>, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let result = sse_request(&client, url, "tools/list", None).await?;
    let tools = result.get("tools").and_then(|t| t.as_array()).ok_or_else(|| "返回结果缺少 tools 字段".to_string())?;
    Ok(tools.iter().map(|t| McpToolInfo {
        name: t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
        description: t.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
        input_schema: t.get("inputSchema").or_else(|| t.get("input_schema")).cloned().unwrap_or(json!({"type": "object"})),
    }).collect())
}

async fn sse_call_tool_with_client(client: &reqwest::Client, url: &str, tool_name: &str, args: serde_json::Value) -> Result<String, String> {
    let params = json!({"name": tool_name, "arguments": args});
    let result = sse_request(client, url, "tools/call", Some(params)).await?;
    Ok(result.get("content").and_then(|c| c.as_array()).map(|arr| arr.iter()
        .filter_map(|item| item.get("text").and_then(|t| t.as_str())).collect::<Vec<&str>>().join("\n")).unwrap_or_default())
}

/// 清理工具名中的特殊字符（DeepSeek 要求只含 [a-zA-Z0-9_-]）
fn sanitize_mcp_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

// ─── 注册到 ToolRegistry ──────────────────────────────

/// 将所有已发现的 MCP 工具注册为独立的 ToolDef
///
/// 命名格式: `mcp__{server_name}__{tool_name}`
/// LLM 可直接调用每个 MCP 工具，handler 自动路由到对应服务器
///
/// 采用**差异更新**策略：只注册新增的、移除已删除的，已存在的工具不重复注册。
pub async fn register_tools(registry: &crate::tools::registry::ToolRegistry) {
    use crate::tools::registry::ToolDef;
    use std::sync::Arc;

    let store = get_store();
    let guard = store.lock().await;

    // 收集所有启用的 MCP 服务器中已发现的工具
    let mut entry_map: HashMap<String, (String, String, String, serde_json::Value)> = HashMap::new();
    for server in &guard.servers {
        if !server.enabled {
            continue;
        }
        for tool in &server.tools {
            let entry_name = format!("mcp__{}__{}", sanitize_mcp_name(&server.name), sanitize_mcp_name(&tool.name));
            let params = if tool.input_schema.is_null() {
                serde_json::json!({"type": "object"})
            } else {
                tool.input_schema.clone()
            };
            entry_map.insert(entry_name, (
                tool.description.clone(),
                server.name.clone(),
                tool.name.clone(),
                params,
            ));
        }
    }
    drop(guard);

    let current_tools: HashSet<String> = entry_map.keys().cloned().collect();
    let registered_tools: HashSet<String> = registry.list_names_by_prefix("mcp__").await.into_iter().collect();

    // 差异更新：移除不再存在的工具
    for removed in registered_tools.difference(&current_tools) {
        tracing::debug!("[MCP] 移除已删除工具: {}", removed);
        registry.remove_by_name(removed).await;
    }

    // 差异更新：只注册新增的工具
    let to_add: Vec<String> = current_tools.difference(&registered_tools).cloned().collect();
    let to_add_count = to_add.len();

    let store_arc = get_store();
    for entry_name in &to_add {
        let (description, server_name, tool_name, input_schema) = entry_map.remove(entry_name).unwrap();
        let srv_name = server_name.clone();
        let t_name = tool_name.clone();
        let store_clone = store_arc.clone();

        let handler = Arc::new(
            move |args: serde_json::Value,
                  _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>|
                  -> Result<String, String> {
                let srv_name = srv_name.clone();
                let t_name = t_name.clone();
                let store_clone = store_clone.clone();
                let tool_args = args;

                let result = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new()
                        .map_err(|e| format!("Runtime error: {}", e))?;
                    rt.block_on(async move {
                        let guard = store_clone.lock().await;
                        let server = guard.get(&srv_name).cloned();
                        drop(guard);
                        let server = server
                            .ok_or_else(|| format!("MCP server '{}' not found", srv_name))?;
                        if !server.enabled {
                            return Err(format!("MCP server '{}' is disabled", srv_name));
                        }
                        crate::mcp::call_tool(&server, &t_name, tool_args).await
                    })
                })
                .join()
                .map_err(|_| "MCP tool call crashed".to_string())??;

                Ok(result)
            },
        );

        // 在描述末尾追加服务器信息，提示 LLM 同一服务器的工具可配合使用
        let full_description = if description.is_empty() {
            format!("(MCP 服务器: {})", server_name)
        } else {
            format!("{} (MCP 服务器: {})", description, server_name)
        };

        let tool_def = ToolDef {
                        name: entry_name.clone(),
            display_name: entry_name.clone(),
            description: full_description,
            parameters: input_schema,
            handler,
        };

        registry.register(tool_def).await;
    }

    if to_add_count > 0 {
        tracing::info!("[MCP] 新增 {} 个 MCP 工具到 ToolRegistry", to_add_count);
    }
    let removed_count = registered_tools.difference(&current_tools).count();
    if removed_count > 0 {
        tracing::info!("[MCP] 移除了 {} 个 MCP 工具", removed_count);
    }
}
