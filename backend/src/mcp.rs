use rmcp::model::{CallToolRequestParams, RawContent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

const LOG_PREFIX: &str = "[MCP] ";

// ─── 类型定义 ────────────────────────────────────────────────

/// MCP 服务器配置
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
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

// ─── 存储 ────────────────────────────────────────────────────

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
        if let Ok(content) = serde_json::to_string_pretty(&self.servers) {
            if let Err(e) = std::fs::write(&self.path, content) {
                tracing::warn!("{}保存 MCP 配置文件失败: {:?}, 错误: {}", LOG_PREFIX, self.path, e);
            } else {
                tracing::debug!("{}保存 MCP 配置文件成功: {:?}", LOG_PREFIX, self.path);
            }
        }
    }

    pub fn list(&self) -> &[McpServerConfig] { &self.servers }
    pub fn get(&self, name: &str) -> Option<&McpServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut McpServerConfig> {
        self.servers.iter_mut().find(|s| s.name == name)
    }
    pub fn add(&mut self, server: McpServerConfig) {
        self.servers.push(server);
        self.save();
    }
    pub fn remove(&mut self, name: &str) -> bool {
        let len = self.servers.len();
        self.servers.retain(|s| s.name != name);
        let removed = self.servers.len() < len;
        if removed { self.save(); }
        removed
    }
    pub fn toggle(&mut self, name: &str) -> bool {
        if let Some(server) = self.get_mut(name) {
            server.enabled = !server.enabled;
            self.save();
            true
        } else { false }
    }
    pub fn update_tools(&mut self, name: &str, tools: Vec<McpToolInfo>) -> bool {
        if let Some(server) = self.get_mut(name) {
            server.tools = tools;
            self.save();
            true
        } else { false }
    }
}

// ─── 全局管理器 ──────────────────────────────────────────────

static MCP_STORE: once_cell::sync::Lazy<Arc<Mutex<McpStore>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(McpStore::load())));

pub fn get_store() -> Arc<Mutex<McpStore>> { MCP_STORE.clone() }

// ─── 工具 ────────────────────────────────────────────────────

/// 连接 MCP 服务器并发现工具
pub async fn discover_tools(server: &McpServerConfig) -> Result<Vec<McpToolInfo>, String> {
    tracing::info!("{}开始发现 MCP 服务器工具: {}, 传输类型: {}", LOG_PREFIX, server.name, server.transport_type);
    
    match server.transport_type.as_str() {
        "stdio" => {
            use rmcp::ServiceExt;
            let cmd = server.command.as_deref().ok_or("缺少启动命令")?;
            let args: Vec<&str> = server.args.as_ref()
                .map(|a| a.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default();
            tracing::debug!("{}创建 MCP 子进程: {} {:?}", LOG_PREFIX, cmd, args);
            
            let mut command = tokio::process::Command::new(cmd);
            command.args(&args);
            let transport = rmcp::transport::child_process::TokioChildProcess::new(command)
                .map_err(|e| {
                    let err = format!("创建 MCP 子进程失败: {}", e);
                    tracing::error!("{}{}", LOG_PREFIX, err);
                    err
                })?;
            let peer = ().serve(transport).await.map_err(|e| {
                let err = format!("连接失败: {}", e);
                tracing::error!("{}{}", LOG_PREFIX, err);
                err
            })?;
            tracing::debug!("{}MCP 服务器连接成功: {}", LOG_PREFIX, server.name);
            
            let r = peer.list_all_tools().await.map_err(|e| {
                let err = format!("获取工具列表失败: {}", e);
                tracing::error!("{}{}", LOG_PREFIX, err);
                err
            })?;
            let _ = peer.cancel().await;
            
            let tools: Vec<McpToolInfo> = r.into_iter().map(|t| McpToolInfo {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                input_schema: serde_json::to_value(&*t.input_schema).unwrap_or_default(),
            }).collect();
            tracing::info!("{}发现 MCP 服务器工具完成: {}, 共 {} 个工具", LOG_PREFIX, server.name, tools.len());
            Ok(tools)
        }
        "streamable-http" => {
            let err = "streamable-http 传输类型暂不支持，请使用 stdio 或 sse 类型".to_string();
            tracing::warn!("{}{}", LOG_PREFIX, err);
            Err(err)
        }
        "sse" => {
            let url = server.url.as_deref().ok_or("缺少 URL")?;
            sse_list_tools(url).await
        }
        t => {
            let err = format!("不支持的传输类型: {}", t);
            tracing::error!("{}{}", LOG_PREFIX, err);
            Err(err)
        }
    }
}

/// 调用 MCP 工具
pub async fn call_tool(
    server: &McpServerConfig,
    tool_name: &str,
    args: serde_json::Value,
) -> Result<String, String> {
    tracing::info!("{}开始调用 MCP 工具: {} -> {}", LOG_PREFIX, server.name, tool_name);
    tracing::debug!("{}调用参数: {}", LOG_PREFIX, args);
    
    match server.transport_type.as_str() {
        "stdio" => {
            use rmcp::ServiceExt;
            let cmd = server.command.as_deref().ok_or("缺少启动命令")?;
            let args_list: Vec<&str> = server.args.as_ref()
                .map(|a| a.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default();
            
            let mut command = tokio::process::Command::new(cmd);
            command.args(&args_list);
            let transport = rmcp::transport::child_process::TokioChildProcess::new(command)
                .map_err(|e| {
                    let err = format!("创建 MCP 子进程失败: {}", e);
                    tracing::error!("{}{}", LOG_PREFIX, err);
                    err
                })?;
            let peer = ().serve(transport).await.map_err(|e| {
                let err = format!("连接失败: {}", e);
                tracing::error!("{}{}", LOG_PREFIX, err);
                err
            })?;

            let json_object: serde_json::Map<String, serde_json::Value> = match args {
                serde_json::Value::Object(map) => map,
                other => {
                    let mut map = serde_json::Map::new();
                    map.insert("value".to_string(), other);
                    map
                }
            };
            let result = peer.call_tool(
                CallToolRequestParams::new(tool_name.to_string()).with_arguments(json_object),
            ).await.map_err(|e| {
                let err = format!("调用工具失败: {}", e);
                tracing::error!("{}{}", LOG_PREFIX, err);
                err
            })?;
            let _ = peer.cancel().await;

            let mut output = String::new();
            for content in result.content {
                match content.raw {
                    RawContent::Text(t) => { output.push_str(&t.text); output.push('\n'); }
                    _ => {}
                }
            }
            let output_str = output.trim().to_string();
            tracing::info!("{}MCP 工具调用完成: {} -> {}, 结果长度: {} 字符", LOG_PREFIX, server.name, tool_name, output_str.len());
            tracing::debug!("{}调用结果: {}", LOG_PREFIX, output_str);
            Ok(output_str)
        }
        "streamable-http" => {
            let err = "streamable-http 传输类型暂不支持，请使用 stdio 或 sse 类型".to_string();
            tracing::warn!("{}{}", LOG_PREFIX, err);
            Err(err)
        }
        "sse" => {
            let url = server.url.as_deref().ok_or("缺少 URL")?;
            sse_call_tool(url, tool_name, args).await
        }
        t => {
            let err = format!("不支持的传输类型: {}", t);
            tracing::error!("{}{}", LOG_PREFIX, err);
            Err(err)
        }
    }
}

// ─── SSE 传输（传统 MCP SSE 协议）─────────────────────────────

/// 通过 SSE 协议向 MCP 服务器发送请求并等待响应
async fn sse_request(
    url: &str,
    method: &str,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    tracing::debug!("{}SSE 请求: {} -> {}", LOG_PREFIX, url, method);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| {
            let err = format!("创建 HTTP 客户端失败: {}", e);
            tracing::error!("{}{}", LOG_PREFIX, err);
            err
        })?;

    let request_id = uuid::Uuid::new_v4().to_string();

    // 构造 JSON-RPC 请求
    let mut req_body = json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": method,
    });
    if let Some(p) = params {
        req_body["params"] = p;
    }

    // 发送 POST 请求
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| {
            let err = format!("SSE POST 请求失败: {}", e);
            tracing::error!("{}{}", LOG_PREFIX, err);
            err
        })?;

    let _content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // 如果是 SSE 响应（或直接 JSON 响应），尝试解析
    let body = resp
        .bytes()
        .await
        .map_err(|e| {
            let err = format!("读取响应失败: {}", e);
            tracing::error!("{}{}", LOG_PREFIX, err);
            err
        })?;

    if body.is_empty() {
        let err = "SSE 返回空响应".to_string();
        tracing::warn!("{}{}", LOG_PREFIX, err);
        return Err(err);
    }

    // 先尝试直接解析为 JSON（某些服务器直接返回 JSON-RPC 响应体）
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) {
        if val.get("jsonrpc").is_some() {
            if let Some(err) = val.get("error") {
                let err_msg = format!(
                    "MCP 错误: {}",
                    err.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误")
                );
                tracing::error!("{}{}", LOG_PREFIX, err_msg);
                return Err(err_msg);
            }
            tracing::debug!("{}SSE 请求成功: {} -> {}", LOG_PREFIX, url, method);
            return Ok(val.get("result").cloned().unwrap_or(val));
        }
    }

    // 尝试按 SSE 格式解析（event: message\ndata: {...}\n\n）
    let text = String::from_utf8_lossy(&body);
    let mut data_parts: Vec<&str> = Vec::new();
    let mut in_data = false;

    for line in text.lines() {
        if line.starts_with("data: ") {
            let data = line.trim_start_matches("data: ");
            // 如果 data 不是完整的 JSON，可能是分块的
            data_parts.push(data);
            in_data = true;
        } else if in_data && !line.starts_with("event:") && !line.is_empty() {
            // 多行 data 的续行
            data_parts.push(line.trim());
        } else if line.is_empty() && in_data {
            // 消息结束，尝试解析
            let data_str = data_parts.join("");
            data_parts.clear();
            in_data = false;

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data_str) {
                if let Some(err) = val.get("error") {
                    let err_msg = format!(
                        "MCP 错误: {}",
                        err.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误")
                    );
                    tracing::error!("{}{}", LOG_PREFIX, err_msg);
                    return Err(err_msg);
                }
                tracing::debug!("{}SSE 请求成功: {} -> {}", LOG_PREFIX, url, method);
                return Ok(val.get("result").cloned().unwrap_or(val));
            }
        }
    }

    // 兜底：如果解析到任何 JSON 就返回
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) {
        return Ok(val);
    }

    let err = format!("无法解析 SSE 响应: {}", &text[..text.len().min(200)]);
    tracing::error!("{}{}", LOG_PREFIX, err);
    Err(err)
}

/// 通过 SSE 协议调用 MCP 工具列表
async fn sse_list_tools(url: &str) -> Result<Vec<McpToolInfo>, String> {
    tracing::debug!("{}SSE 发现工具: {}", LOG_PREFIX, url);
    
    let result = sse_request(url, "tools/list", None).await?;

    let tools = result
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| {
            let err = "返回结果缺少 tools 字段".to_string();
            tracing::error!("{}{}", LOG_PREFIX, err);
            err
        })?;

    let tool_list: Vec<McpToolInfo> = tools
        .iter()
        .map(|t| McpToolInfo {
            name: t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
            description: t
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string(),
            input_schema: t
                .get("inputSchema")
                .or_else(|| t.get("input_schema"))
                .cloned()
                .unwrap_or(json!({"type": "object"})),
        })
        .collect();
    
    tracing::debug!("{}SSE 发现工具完成: {} 个工具", LOG_PREFIX, tool_list.len());
    Ok(tool_list)
}

/// 通过 SSE 协议调用 MCP 工具
async fn sse_call_tool(
    url: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> Result<String, String> {
    tracing::debug!("{}SSE 调用工具: {} -> {}", LOG_PREFIX, url, tool_name);
    
    let params = json!({
        "name": tool_name,
        "arguments": args,
    });

    let result = sse_request(url, "tools/call", Some(params)).await?;

    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<&str>>()
                .join("\n")
        })
        .unwrap_or_default();

    tracing::debug!("{}SSE 调用工具完成: {} -> {}, 结果长度: {} 字符", LOG_PREFIX, url, tool_name, content.len());
    Ok(content)
}
