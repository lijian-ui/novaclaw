# Rust 后端工具系统开发方案

> **文档版本**: v2.0
> **目标语言**: Rust (2021 Edition)
> **适用框架**: Axum + Tokio + Tauri
> **客户端**: 桌面端(Tauri) + Web端(Axum HTTP/WebSocket)
> **状态**: 设计阶段

---

## 目录

1. [系统架构设计](#1-系统架构设计)
2. [核心模块划分](#2-核心模块划分)
3. [工具生命周期管理流程](#3-工具生命周期管理流程)
4. [工具暴露策略](#4-工具暴露策略)
5. [工具发现机制实现](#5-工具发现机制实现)
6. [API 接口规范](#6-api-接口规范)
7. [Tauri 桌面端集成](#7-tauri-桌面端集成)
8. [WebSocket 实时预览](#8-websocket-实时预览)
9. [错误处理机制](#9-错误处理机制)
10. [性能优化策略](#10-性能优化策略)
11. [安全防护措施](#11-安全防护措施)
12. [测试方案](#12-测试方案)
13. [部署流程](#13-部署流程)

---

## 1. 系统架构设计

### 1.1 架构概览

```mermaid
flowchart TB
    subgraph Clients["客户端"]
        Tauri[Tauri Desktop]
        Web[Web Browser]
        CLI[CLI]
    end
    
    subgraph Communication["通信层"]
        invoke[Tauri Invoke]
        http_api[HTTP API]
        ws_api[WebSocket API]
    end
    
    subgraph Core["核心层"]
        tool_registry[ToolRegistry]
        tool_manager[ToolManager]
        policy_engine[PolicyEngine]
        discovery[DiscoveryService]
        event_bus[EventBus]
    end
    
    subgraph Storage["存储层"]
        postgres[PostgreSQL]
        filesystem[Filesystem]
        memory[In-Memory Cache]
    end
    
    subgraph Plugins["插件层"]
        mcp_tools[MCP Tools]
        dynamic_tools[Dynamic Tools]
        builtin_tools[Built-in Tools]
    end
    
    Tauri --> invoke
    Tauri --> ws_api
    Web --> http_api
    Web --> ws_api
    CLI --> http_api
    
    invoke --> Core
    http_api --> Core
    ws_api --> Core
    
    Core --> Storage
    Core --> Plugins
```

### 1.2 设计原则

| 原则 | 说明 |
|------|------|
| **分层架构** | 清晰的职责边界，便于维护和扩展 |
| **策略模式** | 工具暴露策略可配置、可扩展 |
| **延迟加载** | 工具按需加载，减少启动时间 |
| **内存缓存** | 工具元数据和策略结果使用内存缓存 |
| **事件驱动** | 工具注册/变更通过事件通知 |
| **沙箱隔离** | 工具执行环境隔离 |
| **双端支持** | 同时支持 Tauri 桌面端和 Web 端 |

### 1.3 核心数据结构

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVersion(pub semver::Version);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub id: ToolId,
    pub name: String,
    pub description: String,
    pub version: ToolVersion,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub category: String,
    pub parameters: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub provider: String,
    pub requires_auth: bool,
    pub is_optional: bool,
    pub default_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub metadata: ToolMetadata,
    pub handler: ToolHandler,
    pub check_fn: Option<Box<dyn Fn() -> bool + Send + Sync + 'static>>,
}

pub type ToolHandler = Box<dyn Fn(ToolInvocation) -> Result<ToolResult, ToolError> + Send + Sync + 'static>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub tool_id: ToolId,
    pub invocation_id: Uuid,
    pub parameters: serde_json::Value,
    pub session_key: Option<String>,
    pub context: ToolContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    pub user_id: Option<String>,
    pub agent_id: Option<String>,
    pub channel: Option<String>,
    pub group_id: Option<String>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub sandboxed: bool,
    pub client_type: ClientType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientType {
    Tauri,
    Web,
    CLI,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub invocation_id: Uuid,
    pub tool_id: ToolId,
    pub status: ToolStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum ToolStatus {
    Success,
    Failed,
    PendingApproval,
    Cancelled,
    Streaming,
}
```

---

## 2. 核心模块划分

### 2.1 模块结构

```plaintext
hclaw-tools/
├── src/
│   ├── lib.rs
│   ├── registry/           # 工具注册中心
│   │   ├── mod.rs
│   │   ├── tool_registry.rs
│   │   ├── tool_store.rs
│   │   └── version_manager.rs
│   ├── discovery/          # 工具发现服务
│   │   ├── mod.rs
│   │   ├── auto_discovery.rs
│   │   ├── metadata_index.rs
│   │   └── dependency_resolver.rs
│   ├── policy/             # 策略引擎
│   │   ├── mod.rs
│   │   ├── policy_engine.rs
│   │   ├── policy_store.rs
│   │   └── rules.rs
│   ├── exposure/           # 工具暴露控制
│   │   ├── mod.rs
│   │   ├── exposure_controller.rs
│   │   ├── trigger_manager.rs
│   │   └── permission_checker.rs
│   ├── execution/          # 工具执行
│   │   ├── mod.rs
│   │   ├── executor.rs
│   │   ├── sandbox.rs
│   │   └── lifecycle.rs
│   ├── cache/              # 内存缓存服务
│   │   ├── mod.rs
│   │   ├── tool_cache.rs
│   │   └── policy_cache.rs
│   ├── api/                # API 层
│   │   ├── mod.rs
│   │   ├── handlers.rs
│   │   ├── middleware.rs
│   │   └── websocket.rs
│   ├── tauri/              # Tauri 集成
│   │   ├── mod.rs
│   │   ├── invoke_handler.rs
│   │   └── plugin.rs
│   ├── events/             # 事件系统
│   │   ├── mod.rs
│   │   ├── event_bus.rs
│   │   └── event_types.rs
│   └── models/             # 数据模型
│       ├── mod.rs
│       ├── tool.rs
│       ├── policy.rs
│       └── invocation.rs
```

### 2.2 模块职责

| 模块 | 职责 | 核心功能 |
|------|------|---------|
| **registry** | 工具注册与存储 | 注册、查询、版本管理 |
| **discovery** | 工具发现 | 自动扫描、元数据索引、依赖解析 |
| **policy** | 策略引擎 | 权限控制、规则匹配、决策评估 |
| **exposure** | 暴露控制 | 触发条件、动态暴露、权限校验 |
| **execution** | 执行引擎 | 工具调用、沙箱隔离、生命周期管理 |
| **cache** | 内存缓存服务 | 工具元数据缓存、策略结果缓存 |
| **api** | API 接口 | REST API、WebSocket 实时通信 |
| **tauri** | Tauri 集成 | 原生 Invoke 处理、桌面端专用逻辑 |
| **events** | 事件系统 | 事件发布、订阅、通知 |

---

## 3. 工具生命周期管理流程

### 3.1 生命周期状态机

```mermaid
stateDiagram-v2
    [*] --> Registered: register()
    Registered --> Available: enable()
    Registered --> [*]: unregister()
    Available --> Unavailable: disable()
    Available --> PendingApproval: invoke() with approval required
    Available --> Executing: invoke()
    Available --> Streaming: invoke() with streaming
    PendingApproval --> Approved: approve()
    PendingApproval --> Rejected: reject()
    Approved --> Executing
    Executing --> Completed: success
    Executing --> Failed: error
    Streaming --> Completed: stream ended
    Streaming --> Failed: stream error
    Completed --> Available: reset()
    Failed --> Available: reset()
    Rejected --> Available: reset()
    Unavailable --> Available: enable()
    Unavailable --> [*]: unregister()
```

### 3.2 生命周期管理代码示例

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ToolLifecycleManager {
    registry: Arc<RwLock<ToolRegistry>>,
    event_bus: Arc<EventBus>,
}

impl ToolLifecycleManager {
    pub async fn register(&self, definition: ToolDefinition) -> Result<ToolId, ToolError> {
        let tool_id = definition.metadata.id.clone();
        
        // 检查版本冲突
        if self.registry.read().await.has_tool(&tool_id).await? {
            let existing_version = self.registry.read().await.get_version(&tool_id).await?;
            if definition.metadata.version <= existing_version {
                return Err(ToolError::VersionConflict {
                    tool_id: tool_id.clone(),
                    existing: existing_version,
                    new: definition.metadata.version,
                });
            }
        }
        
        // 注册工具
        self.registry.write().await.register(definition).await?;
        
        // 发布注册事件
        self.event_bus.publish(ToolEvent::Registered { tool_id: tool_id.clone() }).await;
        
        Ok(tool_id)
    }
    
    pub async fn enable(&self, tool_id: &ToolId) -> Result<(), ToolError> {
        self.registry.write().await.enable(tool_id).await?;
        self.event_bus.publish(ToolEvent::Enabled { tool_id: tool_id.clone() }).await;
        Ok(())
    }
    
    pub async fn disable(&self, tool_id: &ToolId) -> Result<(), ToolError> {
        self.registry.write().await.disable(tool_id).await?;
        self.event_bus.publish(ToolEvent::Disabled { tool_id: tool_id.clone() }).await;
        Ok(())
    }
    
    pub async fn unregister(&self, tool_id: &ToolId) -> Result<(), ToolError> {
        self.registry.write().await.unregister(tool_id).await?;
        self.event_bus.publish(ToolEvent::Unregistered { tool_id: tool_id.clone() }).await;
        Ok(())
    }
}
```

---

## 4. 工具暴露策略

### 4.1 触发条件

| 触发类型 | 条件 | 说明 |
|---------|------|------|
| **Always** | 无条件 | 始终暴露 |
| **Channel** | 特定渠道 | 如 Slack、Discord |
| **Model** | 特定模型 | 如 GPT-4、Claude |
| **UserRole** | 用户角色 | 如 Owner、Admin |
| **Group** | 群组权限 | 基于群组配置 |
| **Context** | 执行上下文 | 如 sandboxed、cron |
| **ClientType** | 客户端类型 | Tauri、Web、CLI |
| **Time** | 时间窗口 | 定时暴露 |

### 4.2 权限控制层次

```mermaid
flowchart TD
    A[请求工具列表] --> B{检查全局策略}
    B -->|拒绝| Z[返回空列表]
    B -->|允许| C{检查代理策略}
    C -->|拒绝| Z
    C -->|允许| D{检查 Profile}
    D -->|拒绝| Z
    D -->|允许| E{检查群组策略}
    E -->|拒绝| Z
    E -->|允许| F{检查客户端类型}
    F -->|拒绝| Z
    F -->|允许| G{检查沙箱策略}
    G -->|拒绝| Z
    G -->|允许| H[返回工具列表]
```

### 4.3 版本管理策略

```rust
pub struct VersionManager {
    registry: Arc<ToolRegistry>,
    cache: Arc<ToolCache>,
}

impl VersionManager {
    pub async fn get_latest_version(&self, tool_id: &ToolId) -> Result<ToolVersion, ToolError> {
        self.registry.get_latest_version(tool_id).await
    }
    
    pub async fn get_all_versions(&self, tool_id: &ToolId) -> Result<Vec<ToolVersion>, ToolError> {
        self.registry.get_all_versions(tool_id).await
    }
    
    pub async fn get_tool_by_version(
        &self, 
        tool_id: &ToolId, 
        version: Option<ToolVersion>
    ) -> Result<ToolDefinition, ToolError> {
        let version = version.unwrap_or_else(|| self.get_latest_version(tool_id).await?);
        self.registry.get_tool(tool_id, &version).await
    }
    
    pub fn is_compatible(required: &semver::VersionReq, available: &semver::Version) -> bool {
        required.matches(available)
    }
}
```

### 4.4 工具暴露控制器

```rust
pub struct ExposureController {
    policy_engine: Arc<PolicyEngine>,
    version_manager: Arc<VersionManager>,
    trigger_manager: Arc<TriggerManager>,
}

impl ExposureController {
    pub async fn get_exposed_tools(&self, context: &ToolContext) -> Result<Vec<ToolMetadata>, ToolError> {
        let all_tools = self.version_manager.get_all_available_tools().await?;
        let filtered = self.policy_engine.filter_tools(all_tools, context).await?;
        let exposed = self.trigger_manager.check_triggers(filtered, context).await?;
        let sorted = self.sort_by_priority(exposed);
        Ok(sorted)
    }
    
    pub async fn should_expose_tool(&self, tool_id: &ToolId, context: &ToolContext) -> Result<bool, ToolError> {
        let tool = self.version_manager.get_latest_tool(tool_id).await?;
        
        if !tool.metadata.default_enabled {
            return Ok(false);
        }
        
        if !self.policy_engine.is_tool_allowed(tool_id, context).await? {
            return Ok(false);
        }
        
        self.trigger_manager.is_triggered(tool_id, context).await
    }
}
```

---

## 5. 工具发现机制实现

### 5.1 自动注册机制

```rust
use std::path::Path;
use tokio::fs;

pub struct AutoDiscovery {
    search_paths: Vec<String>,
    registry: Arc<ToolRegistry>,
    dependency_resolver: Arc<DependencyResolver>,
}

impl AutoDiscovery {
    pub async fn discover(&self) -> Result<Vec<ToolId>, ToolError> {
        let mut discovered = Vec::new();
        
        for path in &self.search_paths {
            let path = Path::new(path);
            if !path.exists() {
                continue;
            }
            
            let entries = fs::read_dir(path).await.map_err(|e| ToolError::DiscoveryFailed {
                path: path.to_string_lossy().to_string(),
                reason: e.to_string(),
            })?;
            
            for entry in entries.flatten() {
                let file_type = entry.file_type().await?;
                if file_type.is_file() {
                    if let Some(tool) = self.try_load_tool(&entry.path()).await? {
                        let tool_id = tool.metadata.id.clone();
                        let dependencies = self.dependency_resolver.resolve(&tool).await?;
                        self.registry.register_with_dependencies(tool, dependencies).await?;
                        discovered.push(tool_id);
                    }
                } else if file_type.is_dir() {
                    let sub_discovery = AutoDiscovery {
                        search_paths: vec![entry.path().to_string_lossy().to_string()],
                        registry: self.registry.clone(),
                        dependency_resolver: self.dependency_resolver.clone(),
                    };
                    discovered.extend(sub_discovery.discover().await?);
                }
            }
        }
        
        Ok(discovered)
    }
    
    async fn try_load_tool(&self, path: &Path) -> Result<Option<ToolDefinition>, ToolError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => self.load_rust_tool(path).await,
            Some("py") => self.load_python_tool(path).await,
            Some("json") => self.load_json_definition(path).await,
            _ => Ok(None),
        }
    }
}
```

### 5.2 元数据索引

```rust
use std::collections::BTreeMap;

pub struct MetadataIndex {
    by_name: BTreeMap<String, Vec<ToolId>>,
    by_category: BTreeMap<String, Vec<ToolId>>,
    by_tag: BTreeMap<String, Vec<ToolId>>,
    by_provider: BTreeMap<String, Vec<ToolId>>,
    full_text: FullTextIndex,
}

impl MetadataIndex {
    pub fn insert(&mut self, metadata: &ToolMetadata) {
        self.by_name
            .entry(metadata.name.clone())
            .or_default()
            .push(metadata.id.clone());
        
        self.by_category
            .entry(metadata.category.clone())
            .or_default()
            .push(metadata.id.clone());
        
        for tag in &metadata.tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .push(metadata.id.clone());
        }
        
        self.by_provider
            .entry(metadata.provider.clone())
            .or_default()
            .push(metadata.id.clone());
        
        self.full_text.insert(metadata);
    }
    
    pub fn search(&self, query: &str) -> Vec<ToolId> {
        self.full_text.search(query)
    }
    
    pub fn filter_by_category(&self, category: &str) -> Vec<ToolId> {
        self.by_category.get(category).cloned().unwrap_or_default()
    }
}
```

---

## 6. API 接口规范

### 6.1 REST API 端点

| 端点 | 方法 | 功能 | 适用客户端 |
|------|------|------|-----------|
| `/api/v1/tools` | GET | 获取工具列表 | Web、CLI |
| `/api/v1/tools/{id}` | GET | 获取工具详情 | Web、CLI |
| `/api/v1/tools/{id}/versions` | GET | 获取工具版本列表 | Web、CLI |
| `/api/v1/tools` | POST | 注册新工具 | CLI、Web |
| `/api/v1/tools/{id}` | PUT | 更新工具定义 | CLI、Web |
| `/api/v1/tools/{id}` | DELETE | 删除工具 | CLI、Web |
| `/api/v1/tools/search` | GET | 搜索工具 | Web、CLI |
| `/api/v1/tools/exposed` | POST | 获取当前上下文暴露的工具 | Web、CLI、Tauri |
| `/api/v1/invocations` | POST | 调用工具（同步） | Web、CLI |
| `/api/v1/invocations/{id}` | GET | 获取调用状态 | Web、CLI、Tauri |
| `/api/v1/invocations/{id}` | PUT | 更新调用（审批/取消） | Web、CLI、Tauri |
| `/api/v1/policies` | GET | 获取策略列表 | Web、CLI |
| `/api/v1/policies` | POST | 创建策略 | CLI、Web |

### 6.2 请求/响应示例

**获取工具列表**:

```http
GET /api/v1/tools?category=web&limit=10&offset=0
```

```json
{
  "data": [
    {
      "id": "tool_web_search",
      "name": "web_search",
      "description": "Search the web for information",
      "version": "1.0.0",
      "category": "web",
      "tags": ["search", "web"],
      "provider": "builtin",
      "requires_auth": false,
      "is_optional": false,
      "default_enabled": true,
      "parameters": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "Search query" },
          "limit": { "type": "integer", "default": 10 }
        },
        "required": ["query"]
      },
      "output_schema": {
        "type": "object",
        "properties": {
          "results": { "type": "array", "items": { "type": "object" } }
        }
      }
    }
  ],
  "total": 1,
  "limit": 10,
  "offset": 0
}
```

**调用工具**:

```http
POST /api/v1/invocations
Content-Type: application/json
```

```json
{
  "tool_id": "tool_web_search",
  "parameters": {
    "query": "rust programming",
    "limit": 5
  },
  "context": {
    "user_id": "user-123",
    "agent_id": "agent-456",
    "channel": "web",
    "client_type": "web",
    "sandboxed": false
  }
}
```

```json
{
  "invocation_id": "uuid-123",
  "tool_id": "tool_web_search",
  "status": "executing",
  "output": null,
  "error": null,
  "metadata": {
    "started_at": "2024-01-01T00:00:00Z"
  }
}
```

---

## 7. Tauri 桌面端集成

### 7.1 Tauri Invoke 处理

```rust
use tauri::{command, State};

#[command]
pub async fn invoke_tool(
    tool_id: String,
    parameters: serde_json::Value,
    context: ToolContext,
    state: State<'_, AppState>,
) -> Result<ToolResult, ToolError> {
    let tool_id = ToolId(tool_id);
    
    // 设置客户端类型为 Tauri
    let mut context = context;
    context.client_type = ClientType::Tauri;
    
    // 创建调用请求
    let invocation = ToolInvocation {
        tool_id: tool_id.clone(),
        invocation_id: Uuid::new_v4(),
        parameters,
        session_key: None,
        context,
    };
    
    // 执行工具
    state.executor.execute(invocation).await
}

#[command]
pub async fn get_exposed_tools(
    context: ToolContext,
    state: State<'_, AppState>,
) -> Result<Vec<ToolMetadata>, ToolError> {
    let mut context = context;
    context.client_type = ClientType::Tauri;
    state.exposure_controller.get_exposed_tools(&context).await
}

#[command]
pub async fn get_invocation_status(
    invocation_id: String,
    state: State<'_, AppState>,
) -> Result<ToolResult, ToolError> {
    let invocation_id = Uuid::parse_str(&invocation_id)?;
    state.executor.get_status(&invocation_id).await
}
```

### 7.2 Tauri Plugin 注册

```rust
use tauri::{plugin::Plugin, AppHandle, Runtime};

pub struct ToolsPlugin<R: Runtime> {
    state: AppState,
}

impl<R: Runtime> Plugin<R> for ToolsPlugin<R> {
    fn name(&self) -> &'static str {
        "hclaw-tools"
    }
    
    fn init(&mut self, app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
        app.manage(self.state.clone());
        Ok(())
    }
    
    fn setup(&mut self, app: &mut tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
        app.invoke_handler(tauri::generate_handler![
            invoke_tool,
            get_exposed_tools,
            get_invocation_status,
        ]);
        Ok(())
    }
}
```

### 7.3 Tauri 前端调用示例

```typescript
// src-tauri/src/lib.rs 中注册的命令
import { invoke } from '@tauri-apps/api/tauri';

// 调用工具
async function invokeTool(toolId: string, parameters: Record<string, unknown>): Promise<ToolResult> {
  return await invoke('invoke_tool', {
    tool_id: toolId,
    parameters: parameters,
    context: {
      user_id: 'current-user-id',
      agent_id: 'current-agent-id',
      client_type: 'tauri',
    },
  });
}

// 获取可用工具
async function getExposedTools(): Promise<ToolMetadata[]> {
  return await invoke('get_exposed_tools', {
    context: {
      user_id: 'current-user-id',
      client_type: 'tauri',
    },
  });
}
```

---

## 8. WebSocket 实时预览

### 8.1 WebSocket 协议设计

#### 消息类型

| 类型 | 方向 | 说明 |
|------|------|------|
| `subscribe` | Client → Server | 订阅事件频道 |
| `unsubscribe` | Client → Server | 取消订阅 |
| `invoke_stream` | Client → Server | 调用工具（流式） |
| `invocation_update` | Server → Client | 调用状态更新 |
| `file_preview` | Server → Client | 文件实时预览数据 |
| `model_response` | Server → Client | 模型对话响应 |
| `error` | Server → Client | 错误消息 |

#### 订阅请求

```json
{
  "type": "subscribe",
  "channels": ["invocation_updates", "file_preview", "model_response"],
  "session_key": "session-123"
}
```

#### 文件预览推送

```json
{
  "type": "file_preview",
  "timestamp": "2024-01-01T00:00:00Z",
  "data": {
    "file_id": "file-abc",
    "content": "file content here...",
    "content_type": "text/markdown",
    "progress": 85,
    "total_size": 102400
  }
}
```

#### 模型响应推送

```json
{
  "type": "model_response",
  "timestamp": "2024-01-01T00:00:00Z",
  "data": {
    "conversation_id": "conv-123",
    "message_id": "msg-456",
    "content": "thinking...",
    "is_complete": false,
    "tool_calls": []
  }
}
```

### 8.2 WebSocket 服务器实现

```rust
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};

pub async fn websocket_handler(
    ws: WebSocket,
    state: State<'_, AppState>,
) {
    let (mut sender, mut receiver) = ws.split();
    
    // 订阅管理器
    let mut subscriptions = Vec::new();
    
    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(_) => break,
        };
        
        match msg {
            Message::Text(text) => {
                let message: WsMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = sender.send(Message::Text(serde_json::to_string(&WsError {
                            error: format!("Invalid message: {}", e),
                        }).unwrap())).await;
                        continue;
                    }
                };
                
                match message.type_.as_str() {
                    "subscribe" => {
                        let channels = message.channels.clone();
                        subscriptions.extend(channels);
                        
                        // 订阅事件总线
                        for channel in channels {
                            state.event_bus.subscribe(&channel, sender.clone()).await;
                        }
                    }
                    
                    "invoke_stream" => {
                        let invocation: ToolInvocation = serde_json::from_value(message.data)?;
                        let stream = state.executor.execute_streaming(invocation).await?;
                        
                        // 流式发送结果
                        tokio::spawn(async move {
                            while let Some(chunk) = stream.next().await {
                                let update = WsUpdate {
                                    type_: "invocation_update".to_string(),
                                    data: chunk,
                                };
                                let _ = sender.send(Message::Text(serde_json::to_string(&update).unwrap())).await;
                            }
                        });
                    }
                    
                    "unsubscribe" => {
                        let channels = message.channels.clone();
                        subscriptions.retain(|c| !channels.contains(c));
                        
                        for channel in channels {
                            state.event_bus.unsubscribe(&channel, &sender).await;
                        }
                    }
                    
                    _ => {
                        let _ = sender.send(Message::Text(serde_json::to_string(&WsError {
                            error: format!("Unknown message type: {}", message.type_),
                        }).unwrap()).await;
                    }
                }
            }
            
            Message::Close(_) => break,
            _ => {}
        }
    }
}
```

### 8.3 客户端 WebSocket 使用

```typescript
// Web 端 WebSocket 连接
class ToolsWebSocket {
  private ws: WebSocket | null = null;
  private handlers: Map<string, (data: unknown) => void> = new Map();

  connect(url: string) {
    this.ws = new WebSocket(url);
    
    this.ws.onopen = () => {
      this.subscribe(['invocation_updates', 'file_preview', 'model_response']);
    };
    
    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      const handler = this.handlers.get(message.type);
      if (handler) {
        handler(message.data);
      }
    };
  }

  subscribe(channels: string[]) {
    if (!this.ws) return;
    
    this.ws.send(JSON.stringify({
      type: 'subscribe',
      channels,
      session_key: 'current-session-key',
    }));
  }

  onFilePreview(callback: (data: FilePreviewData) => void) {
    this.handlers.set('file_preview', callback);
  }

  onModelResponse(callback: (data: ModelResponseData) => void) {
    this.handlers.set('model_response', callback);
  }
}

// 使用示例
const ws = new ToolsWebSocket();
ws.connect('ws://localhost:8080/ws');

ws.onFilePreview((data) => {
  console.log('File preview:', data.content);
});

ws.onModelResponse((data) => {
  console.log('Model response:', data.content);
});
```

---

## 9. 错误处理机制

### 9.1 错误类型定义

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {tool_id}")]
    ToolNotFound { tool_id: ToolId },
    
    #[error("Version conflict for {tool_id}: existing {existing}, new {new}")]
    VersionConflict {
        tool_id: ToolId,
        existing: ToolVersion,
        new: ToolVersion,
    },
    
    #[error("Circular dependency detected: {tool_id}")]
    CircularDependency { tool_id: ToolId },
    
    #[error("Discovery failed for {path}: {reason}")]
    DiscoveryFailed { path: String, reason: String },
    
    #[error("Tool not available: {tool_id}")]
    ToolNotAvailable { tool_id: ToolId },
    
    #[error("Permission denied for {tool_id}")]
    PermissionDenied { tool_id: ToolId },
    
    #[error("Approval required for {tool_id}")]
    ApprovalRequired { tool_id: ToolId },
    
    #[error("Execution failed for {tool_id}: {reason}")]
    ExecutionFailed { tool_id: ToolId, reason: String },
    
    #[error("Invalid parameters for {tool_id}: {reason}")]
    InvalidParameters { tool_id: ToolId, reason: String },
    
    #[error("Policy evaluation failed: {reason}")]
    PolicyEvaluationFailed { reason: String },
    
    #[error("Database error: {reason}")]
    DatabaseError { reason: String },
    
    #[error("WebSocket error: {reason}")]
    WebSocketError { reason: String },
    
    #[error("Internal server error")]
    InternalError,
}

impl From<ToolError> for axum::http::StatusCode {
    fn from(e: ToolError) -> Self {
        match e {
            ToolError::ToolNotFound { .. } | ToolError::ToolNotAvailable { .. } => Self::NOT_FOUND,
            ToolError::PermissionDenied { .. } | ToolError::ApprovalRequired { .. } => Self::FORBIDDEN,
            ToolError::InvalidParameters { .. } => Self::BAD_REQUEST,
            ToolError::VersionConflict { .. } | ToolError::CircularDependency { .. } => Self::CONFLICT,
            _ => Self::INTERNAL_SERVER_ERROR,
        }
    }
}
```

### 9.2 错误处理中间件

```rust
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use tower::ServiceBuilder;

pub async fn error_middleware<B>(
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let response = next.run(request).await;
    
    if response.status().is_server_error() {
        tracing::error!("Server error occurred: {:?}", response);
    }
    
    Ok(response)
}

pub fn build_middleware_stack() -> ServiceBuilder<()> {
    ServiceBuilder::new()
        .layer(axum::middleware::from_fn(error_middleware))
        .layer(tower_http::trace::TraceLayer::new_for_http())
}
```

---

## 10. 性能优化策略

### 10.1 内存缓存策略

```rust
use std::sync::RwLock;
use std::time::{Duration, Instant};

pub struct ToolCache {
    tools: RwLock<HashMap<ToolId, CachedTool>>,
    policies: RwLock<HashMap<String, CachedPolicy>>,
    ttl: Duration,
}

struct CachedTool {
    metadata: ToolMetadata,
    timestamp: Instant,
}

struct CachedPolicy {
    result: serde_json::Value,
    timestamp: Instant,
}

impl ToolCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            policies: RwLock::new(HashMap::new()),
            ttl,
        }
    }
    
    pub fn get_tool_metadata(&self, tool_id: &ToolId) -> Option<ToolMetadata> {
        let tools = self.tools.read().unwrap();
        tools.get(tool_id).and_then(|cached| {
            if cached.timestamp.elapsed() < self.ttl {
                Some(cached.metadata.clone())
            } else {
                None
            }
        })
    }
    
    pub fn set_tool_metadata(&self, tool_id: &ToolId, metadata: &ToolMetadata) {
        let mut tools = self.tools.write().unwrap();
        tools.insert(tool_id.clone(), CachedTool {
            metadata: metadata.clone(),
            timestamp: Instant::now(),
        });
    }
    
    pub fn invalidate_tool_metadata(&self, tool_id: &ToolId) {
        let mut tools = self.tools.write().unwrap();
        tools.remove(tool_id);
    }
}
```

### 10.2 延迟加载

```rust
use tokio::sync::OnceCell;

pub struct LazyToolLoader {
    loader: OnceCell<ToolDefinition>,
    load_fn: Box<dyn Fn() -> Result<ToolDefinition, ToolError> + Send + Sync + 'static>,
}

impl LazyToolLoader {
    pub fn new(load_fn: impl Fn() -> Result<ToolDefinition, ToolError> + Send + Sync + 'static) -> Self {
        Self {
            loader: OnceCell::new(),
            load_fn: Box::new(load_fn),
        }
    }
    
    pub async fn get(&self) -> Result<&ToolDefinition, ToolError> {
        self.loader.get_or_try_init(|| async { (self.load_fn)() }).await
    }
}
```

### 10.3 批量操作优化

```rust
pub async fn batch_get_tools(
    tool_ids: &[ToolId],
    registry: &ToolRegistry,
    cache: &ToolCache,
) -> Result<HashMap<ToolId, ToolDefinition>, ToolError> {
    let mut results = HashMap::new();
    let mut missing_ids = Vec::new();
    
    // 先从缓存获取
    for tool_id in tool_ids {
        if let Some(metadata) = cache.get_tool_metadata(tool_id) {
            // 从 registry 获取完整定义
            if let Ok(tool) = registry.get_tool(tool_id, &metadata.version).await {
                results.insert(tool_id.clone(), tool);
            }
        } else {
            missing_ids.push(tool_id.clone());
        }
    }
    
    // 批量从数据库获取缺失的工具
    if !missing_ids.is_empty() {
        let db_results = registry.batch_get_tools(&missing_ids).await?;
        for (tool_id, tool) in db_results {
            cache.set_tool_metadata(&tool_id, &tool.metadata);
            results.insert(tool_id, tool);
        }
    }
    
    Ok(results)
}
```

---

## 11. 安全防护措施

### 11.1 权限控制

```rust
pub struct PermissionChecker {
    policy_engine: Arc<PolicyEngine>,
}

impl PermissionChecker {
    pub async fn check_permission(
        &self,
        tool_id: &ToolId,
        context: &ToolContext,
    ) -> Result<bool, ToolError> {
        if context.user_id.is_none() {
            return Ok(false);
        }
        
        let tool = self.policy_engine.get_tool(tool_id).await?;
        if tool.metadata.requires_auth && context.user_id.is_none() {
            return Ok(false);
        }
        
        self.policy_engine.is_tool_allowed(tool_id, context).await
    }
}
```

### 11.2 沙箱隔离

```rust
pub struct SandboxManager {
    container_runtime: Arc<ContainerRuntime>,
}

impl SandboxManager {
    pub async fn execute_in_sandbox(
        &self,
        invocation: ToolInvocation,
    ) -> Result<ToolResult, ToolError> {
        let container = self.container_runtime.create_container().await?;
        
        container.set_resource_limits(ResourceLimits {
            cpu_cores: 1,
            memory_mb: 512,
            disk_mb: 1024,
            network_access: true,
        }).await?;
        
        if let Some(workspace) = invocation.context.workspace_dir.clone() {
            container.mount_directory(workspace, "/workspace").await?;
        }
        
        let result = container.execute(invocation).await;
        container.destroy().await?;
        
        result
    }
}
```

---

## 12. 测试方案

### 12.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tool_registration() {
        let registry = ToolRegistry::new();
        let metadata = ToolMetadata {
            id: ToolId("test_tool".to_string()),
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            version: ToolVersion(semver::Version::new(1, 0, 0)),
            author: None,
            tags: vec!["test".to_string()],
            category: "test".to_string(),
            parameters: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            provider: "test".to_string(),
            requires_auth: false,
            is_optional: false,
            default_enabled: true,
        };
        
        let definition = ToolDefinition {
            metadata,
            handler: Box::new(|_| Ok(ToolResult {
                invocation_id: Uuid::new_v4(),
                tool_id: ToolId("test_tool".to_string()),
                status: ToolStatus::Success,
                output: None,
                error: None,
                metadata: HashMap::new(),
            })),
            check_fn: None,
        };
        
        let result = registry.register(definition).await;
        assert!(result.is_ok());
    }
}
```

### 12.2 集成测试

```rust
#[cfg(test)]
mod integration_tests {
    use axum::{http::StatusCode, routing::post, Router};
    use serde_json::json;
    use tower::ServiceExt;
    
    #[tokio::test]
    async fn test_api_tool_invocation() {
        let app = Router::new()
            .route("/api/v1/invocations", post(invoke_handler))
            .layer(build_middleware_stack());
        
        let request = axum::http::Request::post("/api/v1/invocations")
            .header("Content-Type", "application/json")
            .body(json!({
                "tool_id": "tool_echo",
                "parameters": {"message": "hello"},
                "context": {"user_id": "test", "client_type": "web"}
            }).to_string())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

---

## 13. 部署流程

### 13.1 依赖安装

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 Tauri CLI
cargo install tauri-cli

# 安装项目依赖
cargo install cargo-make
cargo make install-deps
```

### 13.2 构建

```bash
# 后端服务开发构建
cargo build

# 后端服务生产构建
cargo build --release

# Tauri 桌面端构建
tauri build
```

### 13.3 配置

```yaml
# config.yaml
server:
  host: 0.0.0.0
  port: 8080

database:
  url: postgresql://user:password@localhost:5432/hclaw
  pool_size: 10

tools:
  search_paths:
    - ./tools
    - ./plugins
  cache_ttl_seconds: 3600

security:
  allow_cors_origins:
    - http://localhost:3000
    - http://localhost:5173
  max_request_size_mb: 10

logging:
  level: info
  format: json

websocket:
  max_frame_size_mb: 10
  ping_interval_seconds: 30
```

### 13.4 启动

```bash
# 开发模式
cargo run

# 生产模式
./target/release/hclaw-tools --config config.yaml

# Tauri 开发模式
tauri dev
```

### 13.5 健康检查

```http
GET /health
```

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime": 3600,
  "components": {
    "database": "healthy",
    "registry": "healthy",
    "websocket": "healthy"
  }
}
```

---

## 附录

### A. 工具注册清单示例

```json
{
  "id": "tool_web_search",
  "name": "web_search",
  "description": "Search the web for information",
  "version": "1.0.0",
  "category": "web",
  "tags": ["search", "web", "information"],
  "provider": "builtin",
  "requires_auth": false,
  "is_optional": false,
  "default_enabled": true,
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Search query string",
        "minLength": 1,
        "maxLength": 1000
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of results",
        "minimum": 1,
        "maximum": 100,
        "default": 10
      }
    },
    "required": ["query"]
  },
  "output_schema": {
    "type": "object",
    "properties": {
      "results": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "title": {"type": "string"},
            "url": {"type": "string", "format": "uri"},
            "summary": {"type": "string"},
            "score": {"type": "number"}
          }
        }
      },
      "total": {"type": "integer"}
    }
  }
}
```

---

---

## 14. Skills 系统与 Tools 系统对比分析

### 14.1 系统架构差异

#### 14.1.1 Skills 系统架构

```mermaid
flowchart TD
    subgraph Skills["Skills 系统"]
        skill_dir[技能目录]
        skill_md[SKILL.md]
        references[references/]
        templates[templates/]
        scripts[scripts/]
    end
    
    subgraph Processing["处理层"]
        metadata_extract[元数据提取]
        frontmatter[YAML Frontmatter]
        content_render[内容渲染]
        template_subst[模板替换]
    end
    
    subgraph Injection["注入层"]
        prompt_inject[提示词注入]
        config_inject[配置注入]
        context_inject[上下文注入]
    end
    
    subgraph Model["语言模型"]
        llm[LLM]
    end
    
    skill_dir --> skill_md
    skill_dir --> references
    skill_dir --> templates
    skill_dir --> scripts
    
    skill_md --> metadata_extract
    skill_md --> frontmatter
    
    metadata_extract --> content_render
    frontmatter --> content_render
    
    content_render --> template_subst
    template_subst --> prompt_inject
    
    prompt_inject --> config_inject
    prompt_inject --> context_inject
    
    config_inject --> llm
    context_inject --> llm
```

#### 14.1.2 Tools 系统架构

```mermaid
flowchart TD
    subgraph Tools["Tools 系统"]
        tool_module[工具模块]
        schema[JSON Schema]
        handler[处理函数]
        registry[工具注册表]
    end
    
    subgraph Processing["处理层"]
        param_validate[参数验证]
        perm_check[权限检查]
        sandbox[沙箱隔离]
    end
    
    subgraph Execution["执行层"]
        executor[工具执行器]
        result_formatter[结果格式化]
    end
    
    subgraph Model["语言模型"]
        llm[LLM]
        tool_call[工具调用]
    end
    
    tool_module --> schema
    tool_module --> handler
    
    schema --> registry
    handler --> registry
    
    registry --> param_validate
    registry --> perm_check
    
    param_validate --> executor
    perm_check --> sandbox
    sandbox --> executor
    
    executor --> result_formatter
    result_formatter --> tool_call
    tool_call --> llm
```

### 14.2 核心功能模块对比

| 功能模块 | Skills 系统 | Tools 系统 |
|---------|------------|-----------|
| **定义方式** | Markdown 文档 + YAML Frontmatter | Python/Rust 代码 + JSON Schema |
| **加载机制** | 文件系统扫描 + 渐进式披露 | 模块注册 + 延迟加载 |
| **执行方式** | 提示词注入，模型解释执行 | 直接函数调用 |
| **参数处理** | 自然语言描述，模型解析 | JSON Schema 验证 |
| **权限控制** | 基于配置的启用/禁用 | 多层策略管道 |
| **隔离机制** | 注入检测 + 内容审核 | 沙箱隔离 + 资源限制 |
| **生命周期** | 按需加载，缓存机制 | 注册/启用/禁用/注销 |

### 14.3 数据流转机制

#### 14.3.1 Skills 数据流转

```mermaid
sequenceDiagram
    participant User as 用户
    participant Agent as Agent
    participant FS as 文件系统
    participant Render as 内容渲染器
    participant LLM as 语言模型
    
    User->>Agent: /skill-name 命令
    Agent->>FS: 扫描技能目录
    FS-->>Agent: 返回技能列表
    Agent->>FS: 读取 SKILL.md
    FS-->>Agent: 返回文件内容
    Agent->>Render: 解析 Frontmatter
    Render-->>Agent: 元数据 + 内容
    Agent->>Render: 模板替换
    Render-->>Agent: 渲染后的内容
    Agent->>LLM: 注入提示词
    LLM-->>Agent: 执行结果
    Agent-->>User: 返回响应
```

#### 14.3.2 Tools 数据流转

```mermaid
sequenceDiagram
    participant User as 用户
    participant Agent as Agent
    participant Registry as 工具注册表
    participant Executor as 执行器
    participant Tool as 工具实现
    
    User->>Agent: 执行工具调用
    Agent->>Registry: 获取工具定义
    Registry-->>Agent: 返回工具 Schema + Handler
    Agent->>Agent: 参数验证 + 权限检查
    Agent->>Executor: 执行工具
    Executor->>Tool: 调用处理函数
    Tool-->>Executor: 返回结果
    Executor-->>Agent: 格式化结果
    Agent-->>User: 返回响应
```

### 14.4 用户交互方式

#### 14.4.1 Skills 交互方式

```typescript
// Hermes Agent 技能调用示例
import { invoke } from '@tauri-apps/api/tauri';

// 通过命令触发
async function invokeSkill(command: string, instruction?: string) {
  return await invoke('invoke_skill', {
    command,
    instruction,
    context: {
      user_id: 'current-user',
      client_type: 'tauri',
    },
  });
}

// 示例：调用 gif-search 技能
const result = await invokeSkill('/gif-search', '找一只可爱的猫');
```

#### 14.4.2 Tools 交互方式

```typescript
// 工具调用示例
import { invoke } from '@tauri-apps/api/tauri';

// 直接调用工具
async function invokeTool(toolId: string, parameters: Record<string, unknown>) {
  return await invoke('invoke_tool', {
    tool_id: toolId,
    parameters,
    context: {
      user_id: 'current-user',
      client_type: 'tauri',
    },
  });
}

// 示例：调用 web_search 工具
const result = await invokeTool('web_search', {
  query: 'rust programming',
  limit: 5,
});
```

### 14.5 技术实现细节对比

#### 14.5.1 定义格式对比

**Skills 定义格式（SKILL.md）**:

```markdown
---
name: gif-search
description: 搜索 GIF 动图并返回
version: 1.0.0
platforms: [macos, linux, windows]
metadata:
  hermes:
    tags: [fun, media]
    config:
      - key: gif_api_key
        description: GIF API 密钥
        default: ""
---

# GIF 搜索技能

## 功能描述
使用 Tenor API 搜索 GIF 动图。

## 使用说明
1. 解析用户查询中的关键词
2. 调用终端工具执行搜索脚本
3. 返回搜索结果链接

## 脚本路径
scripts/search.js
```

**Tools 定义格式**:

```python
# tools/web_search.py
from tools.registry import registry

def web_search(query: str, limit: int = 10) -> dict:
    """
    Search the web for information.
    
    Args:
        query: Search query string
        limit: Maximum number of results (default: 10)
    
    Returns:
        Search results with title, url, and summary
    """
    # 实现逻辑...

registry.register(
    name="web_search",
    description="Search the web for information",
    parameters={
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Search query"},
            "limit": {"type": "integer", "default": 10}
        },
        "required": ["query"]
    },
    handler=web_search
)
```

#### 14.5.2 加载机制对比

**Skills 加载机制**:

```python
# hermes-agent/agent/skill_utils.py
def discover_all_skill_config_vars() -> List[Dict[str, Any]]:
    """扫描所有启用的技能并收集配置变量声明"""
    all_vars: List[Dict[str, Any]] = []
    seen_keys: set = set()
    
    disabled = get_disabled_skill_names()
    for skills_dir in get_all_skills_dirs():
        if not skills_dir.is_dir():
            continue
        for skill_file in iter_skill_index_files(skills_dir, "SKILL.md"):
            try:
                raw = skill_file.read_text(encoding="utf-8")
                frontmatter, _ = parse_frontmatter(raw)
            except Exception:
                continue
            
            skill_name = frontmatter.get("name") or skill_file.parent.name
            if str(skill_name) in disabled:
                continue
            if not skill_matches_platform(frontmatter):
                continue
            
            config_vars = extract_skill_config_vars(frontmatter)
            for var in config_vars:
                if var["key"] not in seen_keys:
                    var["skill"] = str(skill_name)
                    all_vars.append(var)
                    seen_keys.add(var["key"])
    
    return all_vars
```

**Tools 加载机制**:

```python
# hermes-agent/tools/registry.py
class ToolRegistry:
    def discover_builtin_tools(self, tools_dir: Optional[Path] = None) -> List[str]:
        """导入内置的自注册工具模块"""
        tools_path = Path(tools_dir) if tools_dir else Path(__file__).resolve().parent
        module_names = [
            f"tools.{path.stem}"
            for path in sorted(tools_path.glob("*.py"))
            if path.name not in {"__init__.py", "registry.py", "mcp_tool.py"}
            and _module_registers_tools(path)
        ]
        
        imported: List[str] = []
        for mod_name in module_names:
            try:
                importlib.import_module(mod_name)
                imported.append(mod_name)
            except Exception as e:
                logger.warning("Could not import tool module %s: %s", mod_name, e)
        return imported
```

### 14.6 运行机制判断

#### 14.6.1 证据分析

**证据 1：独立的注册机制**

Skills 使用文件系统扫描和 Markdown 解析，Tools 使用 Python 模块注册和 JSON Schema。两者没有共享的注册中心。

**证据 2：独立的执行路径**

Skills 通过提示词注入执行，Tools 通过函数调用执行。执行引擎完全独立。

**证据 3：独立的配置管理**

Skills 配置存储在 `skills.config.*` 路径下，Tools 配置存储在 `tools.*` 路径下。

**证据 4：独立的权限控制**

Skills 使用简单的启用/禁用列表，Tools 使用多层策略管道。

**证据 5：相互引用关系**

Skills 系统可以调用 Tools（如终端工具执行脚本），但 Tools 系统不依赖 Skills。

#### 14.6.2 结论

**Skills 和 Tools 是两套独立运行的系统**，但存在单向依赖关系：Skills 可以调用 Tools，但 Tools 不依赖 Skills。

### 14.7 优势与适用场景

| 场景 | Skills 系统 | Tools 系统 |
|------|------------|-----------|
| **复杂任务流程** | ✅ 适合描述多步骤任务 | ❌ 不适合 |
| **原子操作** | ❌ 过于笨重 | ✅ 适合 |
| **快速原型** | ✅ 无需代码 | ❌ 需要编程 |
| **性能要求** | ❌ 模型解析开销 | ✅ 直接执行 |
| **安全性** | ⚠️ 需要注入检测 | ✅ 沙箱隔离 |
| **可维护性** | ✅ 文档即代码 | ⚠️ 需要代码维护 |
| **扩展性** | ✅ 易于添加 | ✅ 需要开发 |

### 14.8 设计建议

基于以上分析，建议在设计 HClaw 工具系统时：

1. **保持独立**: Skills 和 Tools 保持独立运行，避免耦合
2. **单向依赖**: 允许 Skills 调用 Tools，但 Tools 不应依赖 Skills
3. **统一接口**: 提供统一的用户交互接口，隐藏底层差异
4. **权限整合**: 建立统一的权限检查层，同时控制两套系统
5. **事件总线**: 使用事件总线实现两套系统间的通信

---

*文档版本: v2.0*
*生成时间: 2026-05-07*
*适用项目: HClaw Tools System*
