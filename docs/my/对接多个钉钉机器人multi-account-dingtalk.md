# 钉钉多账号支持实施方案

> 本文档描述如何在 NovaClaw 后端实现**单 Agent 对接多个钉钉机器人**的功能。
>
> 参考项目：
> - [dingtalk-openclaw-connector](https://github.com/openclaw/dingtalk-openclaw-connector) — OpenClaw 框架的多账号实现
> - [hermes-agent](https://github.com/revolsys/hermes-agent) — Python 版本的单适配器单机器人模式

---

## 1. 概述

### 1.1 目标

在 NovaClaw 后端实现类似 OpenClaw 的多账号机制：

- **同一进程内**管理多个钉钉机器人（账号）
- 所有机器人**共享 Agent 能力**（共享 LLM、工具、记忆等）
- 每个账号有**独立的配置**（clientId/clientSecret、策略、名称）
- 同一 `clientId` 只建立**一个 WebSocket 连接**（去重）
- 会话按 `accountId + chat_id` **隔离**

### 1.2 术语定义

| 术语 | 说明 |
|------|------|
| **Account（账号）** | 一个钉钉机器人实例，拥有独立的 clientId/clientSecret |
| **accountId** | 账号唯一标识符，用于在配置和代码中引用特定账号 |
| **Adapter** | 适配器，一个账号对应一个 DingTalkAdapter 实例 |
| **Platform** | 平台类型（如 DingTalk），一个平台可有多个账号 |
| **Session** | 会话，由 `accountId + conversationId` 唯一标识 |

---

## 2. 当前架构分析

### 2.1 NovaClaw 现有架构

```
lib.rs::initialize()
├── IMGateway::new()
└── for channel in im_config.channels {
    └── if dingtalk + stream_mode:
        ├── DingTalkClient::new(cid, cs)
        ├── DingTalkAdapter::new(client)
        └── gateway.register(adapter)
}
```

**问题**：
- 当前 `IMGateway` 按 `PlatformType`（如 `DingTalk`）注册适配器
- 同一平台只能注册**一个**适配器实例
- 不支持多账号

### 2.2 dingtalk-openclaw-connector 架构

```
channel.ts::startAccount(accountId)
├── resolveDingtalkAccount(cfg, accountId)  ← 解析账号配置
├── monitorDingtalkProvider({ accountId })  ← 启动连接
└── dwsCredentialsByAccount.set(accountId) ← 凭据隔离
```

**关键设计**：
- `PlatformRegistry` 按 `accountId` 而非 `PlatformType` 管理连接
- 每个账号独立启动 `monitorDingtalkProvider`
- 凭据存储在 `Map<accountId, {clientId, clientSecret}>` 中隔离

### 2.3 架构对比

| 维度 | NovaClaw（当前） | dingtalk-openclaw-connector |
|------|-----------------|----------------------------|
| 适配器注册键 | `PlatformType` | `accountId` |
| 连接管理 | 单连接 | 每个账号独立连接 |
| 凭据隔离 | 无（单连接） | `Map<accountId, creds>` |
| 会话隔离 | `platform + conversation_id` | `accountId + conversation_id` |
| 多账号配置 | 无 | `accounts: { bot1: {...}, bot2: {...} }` |

---

## 3. 目标架构

### 3.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         IMGateway                               │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              AccountRegistry<accountId, Adapter>           │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │ bot1        │  │ bot2        │  │ bot3        │  ...   │   │
│  │  │ DingTalk    │  │ DingTalk    │  │ DingTalk    │       │   │
│  │  │ Adapter     │  │ Adapter     │  │ Adapter     │       │   │
│  │  │             │  │             │  │             │       │   │
│  │  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │       │   │
│  │  │ │Client   │ │  │ │Client   │ │  │ │Client   │ │       │   │
│  │  │ │实例     │ │  │ │实例     │ │  │ │实例     │ │       │   │
│  │  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      SessionSource                              │
│  accountId + platform + conversationId + senderId              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     AgentRuntime（共享）                         │
│  LLM Client + ToolRegistry + MemoryStore + Skills               │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 核心组件变更

| 组件 | 变更说明 |
|------|----------|
| `im::config` | 支持 `accounts` 数组，支持 defaultAccount |
| `im::registry` | 从 `HashMap<PlatformType, ...>` 改为 `HashMap<accountId, ...>` |
| `im::types` | `SessionSource` 增加 `account_id` 字段 |
| `im::gateway` | 按 accountId 路由消息和回复 |
| `dingtalk::adapter` | 接受 accountId 参数 |
| `dingtalk::client` | 接受 accountId 参数（用于日志和调试） |
| `lib.rs` | 循环启动多个账号的连接 |

---

## 4. 实施计划

### 4.1 阶段一：数据模型改造

#### 4.1.1 扩展配置结构

```rust
// im/config.rs

/// IM 渠道配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMChannelConfig {
    pub id: String,           // 渠道唯一标识
    pub name: String,         // 渠道显示名称
    pub channel_type: String, // "dingtalk"
    pub enabled: bool,
    pub default_account: Option<String>, // 默认账号 ID
    pub config: IMChannelDetail,
}

/// 账号配置（新增）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,           // 账号唯一标识
    pub name: Option<String>, // 账号显示名称
    pub enabled: bool,
    pub credentials: AccountCredentials,
    pub policies: AccountPolicies,
}

/// 账号凭据（新增）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCredentials {
    pub client_id: String,
    pub client_secret: String,
}

/// 账号策略（新增）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPolicies {
    pub dm_policy: DmPolicy,      // 私聊策略
    pub group_policy: GroupPolicy, // 群聊策略
    pub allow_from: Option<Vec<String>>, // 允许的发送者
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DmPolicy {
    Open,      // 开放，任何人都可以私聊
    Pairing,   // 需要配对
    Allowlist, // 仅白名单
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupPolicy {
    Open,      // 开放，任何群都可使用
    Allowlist, // 仅白名单群
}
```

#### 4.1.2 扩展消息类型

```rust
// im/types.rs

/// 跨平台会话来源标识（增加 accountId）
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSource {
    pub account_id: String,    // 新增：账号 ID
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
}

/// 标准化入站消息（增加 accountId）
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: String,
    pub account_id: String,    // 新增：来源账号
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
    pub sender_staff_id: Option<String>,
    pub sender_name: Option<String>,
    pub text: String,
    pub media_urls: Vec<String>,
    pub raw: serde_json::Value,
    pub session_webhook: Option<String>,
    pub conversation_type: ConversationType,
    pub conversation_title: Option<String>,
    pub timestamp: i64,
}
```

### 4.2 阶段二：适配器注册表改造

#### 4.2.1 AccountRegistry 设计

```rust
// im/registry.rs

use crate::im::adapter::IMAdapter;
use crate::im::types::PlatformType;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 账号信息
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub account_id: String,
    pub platform: PlatformType,
    pub adapter: Arc<dyn IMAdapter>,
    pub enabled: bool,
    pub name: Option<String>,
}

/// 账号注册中心
///
/// 替代原有的 PlatformRegistry，按 accountId 管理适配器实例。
/// 支持同一平台的多个账号。
pub struct AccountRegistry {
    accounts: RwLock<HashMap<String, Arc<dyn IMAdapter>>>,
    /// accountId → AccountInfo 的映射（包含元数据）
    account_info: RwLock<HashMap<String, AccountInfo>>,
}

impl AccountRegistry {
    pub fn new() -> Self {
        Self {
            accounts: RwLock::new(HashMap::new()),
            account_info: RwLock::new(HashMap::new()),
        }
    }

    /// 注册账号适配器
    pub async fn register_account(&self, info: AccountInfo) {
        tracing::info!(
            "注册账号适配器: {} (platform={}, name={:?})",
            info.account_id,
            info.platform,
            info.name
        );
        let adapter = info.adapter.clone();
        self.accounts.write().await.insert(info.account_id.clone(), adapter);
        self.account_info.write().await.insert(info.account_id.clone(), info);
    }

    /// 获取指定账号的适配器
    pub async fn get(&self, account_id: &str) -> Option<Arc<dyn IMAdapter>> {
        self.accounts.read().await.get(account_id).cloned()
    }

    /// 获取账号信息
    pub async fn get_info(&self, account_id: &str) -> Option<AccountInfo> {
        self.account_info.read().await.get(account_id).cloned()
    }

    /// 检查账号是否已连接
    pub async fn is_connected(&self, account_id: &str) -> bool {
        self.accounts
            .read()
            .await
            .get(account_id)
            .map(|a| a.is_connected())
            .unwrap_or(false)
    }

    /// 获取所有已注册的账号 ID
    pub async fn account_ids(&self) -> Vec<String> {
        let guard = self.accounts.read().await;
        guard.keys().cloned().collect()
    }

    /// 按平台类型获取所有账号
    pub async fn get_by_platform(&self, platform: &PlatformType) -> Vec<AccountInfo> {
        let guard = self.account_info.read().await;
        guard
            .values()
            .filter(|info| info.platform == *platform)
            .cloned()
            .collect()
    }

    /// 移除账号
    pub async fn unregister(&self, account_id: &str) {
        tracing::info!("注销账号: {}", account_id);
        self.accounts.write().await.remove(account_id);
        self.account_info.write().await.remove(account_id);
    }
}
```

### 4.3 阶段三：IMGateway 改造

#### 4.3.1 消息路由增强

```rust
// im/gateway.rs

impl IMGateway {
    /// 按 accountId 回复消息（精确路由）
    pub async fn reply_to_account(
        &self,
        account_id: &str,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let adapter = self
            .registry
            .get(account_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("账号未注册: {}", account_id)))?;
        adapter.reply(original, text).await
    }

    /// 向指定账号发送消息
    pub async fn send_to_account(
        &self,
        account_id: &str,
        target: &MessageTarget,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let adapter = self
            .registry
            .get(account_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("账号未注册: {}", account_id)))?;
        adapter.send_text(target, text).await
    }

    /// 获取所有已连接账号
    pub async fn connected_accounts(&self) -> Vec<String> {
        let account_ids = self.registry.account_ids().await;
        account_ids
            .into_iter()
            .filter(|id| self.registry.is_connected(id).await)
            .collect()
    }
}
```

#### 4.3.2 会话源生成增强

```rust
// im/session.rs

/// 从入站消息生成会话源（包含 accountId）
pub fn session_source_from_incoming(msg: &IncomingMessage) -> SessionSource {
    SessionSource {
        account_id: msg.account_id.clone(),
        platform: msg.platform.clone(),
        conversation_id: msg.conversation_id.clone(),
        sender_id: msg.sender_id.clone(),
    }
}

/// 获取会话标识（用于存储和查找）
pub fn session_key(source: &SessionSource) -> String {
    format!(
        "{}:{}:{}",
        source.account_id, source.platform, source.conversation_id
    )
}
```

### 4.4 阶段四：钉钉模块改造

#### 4.4.1 Client 增加 accountId

```rust
// dingtalk/mod.rs

/// 钉钉客户端（统一外观）
pub struct DingTalkClient {
    pub connection: DingTalkConnection,
    pub message_sender: MessageSender,
    pub card_sender: card::CardSender,
    pub handler_registry: Arc<HandlerRegistry>,
    pub client_id: String,
    pub account_id: String,  // 新增：账号标识
    pub account_name: Option<String>,  // 新增：账号名称
}

impl DingTalkClient {
    /// 创建 DingTalk 客户端（支持多账号）
    pub async fn new(
        account_id: String,
        account_name: Option<String>,
        client_id: String,
        client_secret: String,
    ) -> Self {
        // ... 初始化逻辑
        Self {
            account_id,  // 保存账号 ID
            account_name,
            // ... 其他字段
        }
    }
}
```

#### 4.4.2 Adapter 增加 accountId

```rust
// dingtalk/adapter.rs

pub struct DingTalkAdapter {
    client: Arc<DingTalkClient>,
    current_card: std::sync::Mutex<Option<(AICardInstance, mpsc::UnboundedSender<String>)>>,
    pub account_id: String,  // 新增
}

impl DingTalkAdapter {
    pub fn new(client: Arc<DingTalkClient>, account_id: String) -> Self {
        Self {
            client,
            account_id,
            current_card: std::sync::Mutex::new(None),
        }
    }
}
```

### 4.5 阶段五：初始化逻辑改造

```rust
// lib.rs

/// 初始化 IM Gateway（支持多账号）
pub async fn initialize() {
    // ... 前置初始化 ...

    let gateway = im::IMGateway::new();
    let im_config = im::config::load();

    for channel in &im_config.channels {
        if !channel.enabled {
            continue;
        }

        match channel.channel_type.as_str() {
            "dingtalk" => {
                if channel.use_stream_mode() {
                    // 获取默认账号或所有账号
                    let account_ids = channel.resolve_account_ids();

                    for account_id in account_ids {
                        let account_config = channel.get_account(&account_id);

                        if !account_config.enabled {
                            tracing::info!("钉钉账号已禁用，跳过: {}", account_id);
                            continue;
                        }

                        // 检查 clientId 去重
                        let client_id = &account_config.credentials.client_id;
                        if is_client_id_already_connected(&gateway, client_id).await {
                            tracing::info!(
                                "clientId {} 已被其他账号使用，跳过: {}",
                                &client_id[..8.min(client_id.len())],
                                account_id
                            );
                            continue;
                        }

                        // 创建账号连接
                        let dt_client = Arc::new(
                            dingtalk::DingTalkClient::new(
                                account_id.clone(),
                                account_config.name.clone(),
                                client_id.clone(),
                                account_config.credentials.client_secret.clone(),
                            )
                            .await,
                        );

                        let dt_adapter = Arc::new(
                            dingtalk::adapter::DingTalkAdapter::new(dt_client.clone(), account_id.clone())
                        );

                        // 注册回调处理器
                        {
                            let incoming_tx = gateway.incoming_tx.clone();
                            let account_id_clone = account_id.clone();
                            dt_client.register_handler(
                                crate::im::handler::IMGatewayCallbackHandler::new(
                                    incoming_tx,
                                    account_id_clone,
                                ),
                            ).await;
                        }

                        // 注册账号
                        gateway.register_account(im::AccountInfo {
                            account_id: account_id.clone(),
                            platform: im::types::PlatformType::DingTalk,
                            adapter: dt_adapter.clone(),
                            enabled: true,
                            name: account_config.name.clone(),
                        }).await;

                        tracing::info!(
                            "钉钉账号已注册: {} (name={:?}, clientId={}...)",
                            account_id,
                            account_config.name,
                            &client_id[..8.min(client_id.len())]
                        );
                    }
                }
            }
            _ => {
                tracing::warn!("不支持的 IM 渠道类型: {}", channel.channel_type);
            }
        }
    }

    // 保存到全局
    {
        let mut g = IM_GATEWAY.write().await;
        *g = Some(gateway);
    }
}

/// 检查 clientId 是否已被其他账号使用
async fn is_client_id_already_connected(
    gateway: &Arc<IMGateway>,
    client_id: &str,
) -> bool {
    let connected_accounts = gateway.connected_accounts().await;
    for acc_id in connected_accounts {
        if let Some(info) = gateway.registry.get_info(&acc_id).await {
            // 需要从 adapter 或其他途径获取 clientId
            // 暂时通过 adapter 上的信息判断
        }
    }
    false
}
```

### 4.6 阶段六：配置解析增强

```rust
// im/config.rs

impl IMChannelConfig {
    /// 获取所有账号 ID（包括默认账号）
    pub fn resolve_account_ids(&self) -> Vec<String> {
        if let Some(accounts) = &self.accounts {
            accounts.keys().cloned().collect()
        } else if self.use_stream_mode() {
            // 兼容旧配置：使用默认账号
            vec![DEFAULT_ACCOUNT_ID.to_string()]
        } else {
            vec![]
        }
    }

    /// 获取指定账号配置
    pub fn get_account(&self, account_id: &str) -> &AccountConfig {
        static DEFAULT_ACCOUNT: OnceCell<AccountConfig> = OnceCell::new();

        if let Some(accounts) = &self.accounts {
            accounts.get(account_id)
        } else {
            // 兼容旧配置：从顶层凭据构造默认账号
            DEFAULT_ACCOUNT.get_or_init(|| AccountConfig {
                id: DEFAULT_ACCOUNT_ID.to_string(),
                name: None,
                enabled: true,
                credentials: AccountCredentials {
                    client_id: self.config.client_id.clone().unwrap_or_default(),
                    client_secret: self.config.client_secret.clone().unwrap_or_default(),
                },
                policies: AccountPolicies::default(),
            })
        }
    }
}
```

---

## 5. 配置文件设计

### 5.1 新的配置格式（向后兼容）

```json
{
  "channels": [
    {
      "id": "dingtalk-main",
      "name": "主助手",
      "channelType": "dingtalk",
      "enabled": true,
      "defaultAccount": "bot1",
      "accounts": {
        "bot1": {
          "id": "bot1",
          "name": "助手1号",
          "enabled": true,
          "credentials": {
            "clientId": "dingxxxxx1",
            "clientSecret": "yyyyy"
          },
          "policies": {
            "dmPolicy": "pairing",
            "groupPolicy": "allowlist"
          }
        },
        "bot2": {
          "id": "bot2",
          "name": "助手2号",
          "enabled": true,
          "credentials": {
            "clientId": "dingxxxxx2",
            "clientSecret": "zzzzz"
          },
          "policies": {
            "dmPolicy": "open",
            "groupPolicy": "open"
          }
        }
      }
    }
  ]
}
```

### 5.2 旧配置格式（向后兼容）

```json
{
  "channels": [
    {
      "id": "dingtalk",
      "name": "钉钉助手",
      "channelType": "dingtalk",
      "enabled": true,
      "config": {
        "clientId": "dingxxxxx",
        "clientSecret": "yyyyy"
      }
    }
  ]
}
```

**兼容逻辑**：
- 如果 `accounts` 存在，优先使用多账号模式
- 如果 `accounts` 不存在但 `config.clientId` 存在，构造默认账号（ID=`default`）

---

## 6. API 接口设计

### 6.1 账号管理 API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/im/accounts` | 列出所有 IM 账号 |
| GET | `/api/im/accounts/:id` | 获取指定账号详情 |
| POST | `/api/im/accounts` | 添加新账号 |
| PUT | `/api/im/accounts/:id` | 更新账号配置 |
| DELETE | `/api/im/accounts/:id` | 删除账号 |
| POST | `/api/im/accounts/:id/enable` | 启用账号 |
| POST | `/api/im/accounts/:id/disable` | 禁用账号 |
| GET | `/api/im/accounts/:id/status` | 获取账号连接状态 |

### 6.2 响应格式

```json
// GET /api/im/accounts
{
  "accounts": [
    {
      "id": "bot1",
      "name": "助手1号",
      "platform": "dingtalk",
      "enabled": true,
      "connected": true,
      "clientId": "dingxxxxx1",
      "policies": {
        "dmPolicy": "pairing",
        "groupPolicy": "allowlist"
      }
    },
    {
      "id": "bot2",
      "name": "助手2号",
      "platform": "dingtalk",
      "enabled": true,
      "connected": false,
      "clientId": "dingxxxxx2",
      "policies": {
        "dmPolicy": "open",
        "groupPolicy": "open"
      }
    }
  ]
}
```

---

## 7. 实现清单

### 7.1 优先级 P0（核心功能）

- [ ] 扩展 `IMChannelConfig` 支持 `accounts` 数组
- [ ] 新增 `AccountConfig`、`AccountCredentials`、`AccountPolicies` 类型
- [ ] 新增 `AccountInfo` 和 `AccountRegistry`（替代 `PlatformRegistry`）
- [ ] `SessionSource` 增加 `account_id` 字段
- [ ] `IncomingMessage` 增加 `account_id` 字段
- [ ] `IMGateway` 使用 `AccountRegistry`
- [ ] `DingTalkClient` 增加 `account_id` 和 `account_name`
- [ ] `DingTalkAdapter` 增加 `account_id`
- [ ] `lib.rs::initialize()` 循环启动多账号
- [ ] 实现 clientId 去重逻辑
- [ ] 配置文件向后兼容

### 7.2 优先级 P1（API 接口）

- [ ] GET `/api/im/accounts` 实现
- [ ] GET `/api/im/accounts/:id` 实现
- [ ] POST `/api/im/accounts` 实现（动态添加账号）
- [ ] PUT `/api/im/accounts/:id` 实现
- [ ] DELETE `/api/im/accounts/:id` 实现
- [ ] POST `/api/im/accounts/:id/enable` 实现
- [ ] POST `/api/im/accounts/:id/disable` 实现
- [ ] GET `/api/im/accounts/:id/status` 实现

### 7.3 优先级 P2（高级功能）

- [ ] 动态重载：热添加/移除账号（无需重启服务）
- [ ] 账号级策略：`dmPolicy`、`groupPolicy` 执行
- [ ] 凭据重置：重新获取 accessToken
- [ ] 账号级日志标签（便于调试）

---

## 8. 参考资料

- [dingtalk-openclaw-connector/src/config/accounts.ts](https://github.com/openclaw/dingtalk-openclaw-connector/blob/main/src/config/accounts.ts) — 多账号配置解析
- [dingtalk-openclaw-connector/src/channel.ts](https://github.com/openclaw/dingtalk-openclaw-connector/blob/main/src/channel.ts) — `startAccount` 启动逻辑
- [hermes-agent/gateway/platforms/dingtalk.py](https://github.com/revolsys/hermes-agent/blob/main/gateway/platforms/dingtalk.py) — Python 版钉钉适配器
- [NovaClaw 后端钉钉模块](./backend/src/dingtalk/) — 当前实现

---

## 9. 变更记录

| 日期 | 版本 | 变更说明 |
|------|------|----------|
| 2026-05-19 | v0.1 | 初始版本 |
