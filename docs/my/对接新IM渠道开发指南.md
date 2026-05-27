# 对接新 IM 渠道开发指南

> 撰写日期：2026-05-27
> 适用版本：Jeeves 多渠道 IM 架构 v2（多账号 + 复合 key 注册）
> 参考实现：DingTalk（WebSocket 双向）、WeChat（长轮询）、Feishu/飞书（本文作为教学案例）

---

## 一、架构概览

Jeeves IM 系统采用 **Adapter → Registry → Gateway** 三层架构，并引入了**复合 key 注册机制**支持同一平台多账号共存。

```
┌──────────────────────────────────────────────────────────────────────┐
│                          IMGateway                                   │
│  (im/gateway.rs)                                                     │
│  消息路由 / 多账号路由 / Agent 会话管理 / 入站消息循环                   │
├──────────────────────────────────────────────────────────────────────┤
│                       AccountRegistry                                 │
│  (im/registry.rs)                                                     │
│  HashMap<"{platform}:{account_id}", Arc<dyn IMAdapter>>              │
│  复合 key 避免不同平台同 account_id 冲突                                │
├──────────┬──────────┬──────────┬──────────┬───────────┬──────────────┤
│ DingTalk │ 个人微信  │  飞书    │ 企业微信  │ Slack     │ ...未来      │
│ adapter  │ adapter  │ adapter  │ adapter  │ adapter   │              │
│ dingtalk/│ weixin/  │ feishu/  │ wecom/   │ slack/    │              │
├──────────┴──────────┴──────────┴──────────┴───────────┴──────────────┤
│                    Agent (ReAct 循环)                                 │
│                    工具: im_push / 入站消息触发 Agent.run_turn()       │
└──────────────────────────────────────────────────────────────────────┘
```

**新增一个渠道核心流程**：
1. 创建 `{platform}/` 模块（client + adapter + connection + event handler）
2. 实现 `IMAdapter` trait
3. 注册回调管道（`IncomingMessage` → IMGateway 的 `incoming_tx` mpsc 通道）
4. 在 `lib.rs` / `im/reload.rs` 中添加渠道注册逻辑
5. 前端 `IMSettings.tsx` 添加配置表单

---

## 二、核心数据模型

所有跨平台消息类型定义在 `backend/src/im/types.rs`。

### 2.1 平台类型

```rust
/// 平台类型枚举
/// 内置常见平台 + Custom(String) 支持扩展
/// 微信等平台使用 Custom("weixin") 即可，无需修改枚举
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformType {
    DingTalk,
    WeChatWork,
    Feishu,
    Slack,
    Discord,
    Telegram,
    /// 自定义平台：如 Custom("weixin")、Custom("lark")
    #[serde(untagged)]
    Custom(String),
}

impl PlatformType {
    /// 注册中心复合 key 使用此方法
    pub fn as_str(&self) -> &str;
    /// 从配置字符串解析：如 "dingtalk" → DingTalk, "weixin" → Custom("weixin")
    pub fn from_str(s: &str) -> Self;
}
```

### 2.2 入站消息

```rust
/// 标准化入站消息
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: String,
    /// 来源账号 ID（多账号模式下唯一标识机器人）
    pub account_id: String,
    /// 来源账号显示名称（如"测试"，前端会话列表显示用）
    pub account_name: Option<String>,
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
    /// 钉钉真实用户 ID（用于卡片投放等需要真实 userid 的场景）
    pub sender_staff_id: Option<String>,
    pub sender_name: Option<String>,
    pub text: String,
    pub media_urls: Vec<String>,
    pub raw: serde_json::Value,
    /// 钉钉独有：快速回复 Webhook
    pub session_webhook: Option<String>,
    pub conversation_type: ConversationType,
    pub conversation_title: Option<String>,
    pub timestamp: i64,
}
```

### 2.3 消息发送目标

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTarget {
    /// 目标账号 ID（路由到具体机器人）
    pub account_id: String,
    pub platform: PlatformType,
    pub conversation_id: String,
    /// 私聊/群聊，决定了 DingTalk 使用不同 API
    pub conversation_type: ConversationType,
}
```

### 2.4 跨平台会话来源标识

```rust
/// 包含 account_id + platform + conversation_id
/// 用于 IMGateway 按账号精确路由
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSource {
    pub account_id: String,
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
}
```

---

## 三、IMAdapter 核心契约

文件：[backend/src/im/adapter.rs](file:///d:\Project\novaclaw\backend\src\im\adapter.rs)

```rust
#[async_trait]
pub trait IMAdapter: Send + Sync {
    fn platform_type(&self) -> PlatformType;
    fn is_connected(&self) -> bool;
    fn capabilities(&self) -> PlatformCapabilities;

    /// 发送文本消息
    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError>;

    /// 发送 Markdown 消息
    async fn send_markdown(&self, target: &MessageTarget, title: &str, text: &str) -> Result<SendResult, AppError>;

    /// 回复原始消息（优先用 session_webhook，兜底走 REST API）
    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError>;

    /// 流式回复（可选），返回 Sender 用于推文本块
    /// 默认降级为非流式 reply
    async fn start_stream_reply(&self, _original: &IncomingMessage) -> Result<mpsc::UnboundedSender<String>, AppError> {
        Err(AppError::External("该平台不支持流式回复".to_string()))
    }

    async fn finish_stream_reply(&self, _original: &IncomingMessage) -> Result<(), AppError> {
        Ok(())
    }
}
```

---

## 四、注册中心：AccountRegistry（复合 key 机制）

文件：[backend/src/im/registry.rs](file:///d:\Project\novaclaw\backend\src\im\registry.rs)

这是与旧版文档**最大不同**之处。旧文档假设 `HashMap<PlatformType, Arc<dyn IMAdapter>>`，但实际使用的是**复合 key** 机制：

```rust
/// 存储 key = "{platform}:{account_id}"
/// 例如：account_key("default", DingTalk) = "dingtalk:default"
fn account_key(account_id: &str, platform: &PlatformType) -> String {
    format!("{}:{}", platform.as_str(), account_id)
}
```

**为什么用复合 key？**
- 同一平台可以注册多个账号（如钉钉"测试"机器人 + "测试2"机器人）
- 不同平台可能使用相同 account_id（如微信也用 `"im_xxx"`），复合 key 避免冲突
- 路由时通过 `account_id + platform` 精确查找适配器

```rust
pub struct AccountRegistry {
    accounts: RwLock<HashMap<String, Arc<dyn IMAdapter>>>,
    account_info: RwLock<HashMap<String, AccountInfo>>,
}

#[derive(Clone)]
pub struct AccountInfo {
    pub account_id: String,
    pub platform: PlatformType,
    pub adapter: Arc<dyn IMAdapter>,
    pub enabled: bool,
    pub name: Option<String>,
}
```

---

## 五、配置模型

文件：[backend/src/im/config.rs](file:///d:\Project\novaclaw\backend\src\im\config.rs)

配置文件位于 `config/im.json`，结构如下：

```json
{
  "channels": [
    {
      "id": "im_1779168106717",
      "name": "测试",
      "channel_type": "dingtalk",
      "enabled": true,
      "config": {
        "clientId": "dingxxx",
        "clientSecret": "xxx",
        "webhook": "",
        "secret": ""
      },
      "accounts": {
        "robot1": {
          "id": "robot1",
          "name": "测试机器人1号",
          "enabled": true,
          "credentials": {
            "clientId": "dingxxx",
            "clientSecret": "xxx"
          },
          "policies": {
            "dmPolicy": "open",
            "groupPolicy": "open"
          }
        }
      },
      "defaultAccount": "robot1"
    }
  ]
}
```

### 关键概念

| 概念 | 说明 |
|------|------|
| **单账号模式** | 无 `accounts` 字段，直接使用 `config` 下的 `clientId`/`clientSecret`，`account_id` 等于渠道 `id` |
| **多账号模式** | 有 `accounts` 字段，每个账号有自己的凭据、策略，`account_id` 为账号 `id` |
| **`enabled_account_ids()`** | 返回所有启用的账号 ID 列表，决定遍历注册数量 |
| **`get_account()`** | 单账号模式下用渠道 `id` 构造虚拟 `AccountConfig`，多账号模式从 `accounts` 中查找 |

### Rust 核心结构

```rust
pub struct IMChannelConfig {
    pub id: String,            // 渠道唯一标识
    pub name: String,          // 渠道显示名称
    pub channel_type: String,  // "dingtalk" | "weixin" | "feishu" | ...
    pub enabled: bool,
    pub default_account: Option<String>,
    pub config: IMChannelDetail,  // 单账号模式通用配置
    pub accounts: Option<HashMap<String, AccountConfig>>,  // 多账号模式
}

pub struct AccountConfig {
    pub id: String,
    pub name: Option<String>,
    pub enabled: bool,
    pub credentials: AccountCredentials,
    pub policies: AccountPolicies,
}
```

---

## 六、消息路由流程

### 6.1 入站（IM → Agent）

以 DingTalk 为例，入站消息经过 4 层传递：

```
钉钉服务器
    ↓ WebSocket
DingTalkClient.ws_read_loop()      ← connection.rs
    ↓ 解析为 CallbackMessageData
IMGatewayCallbackHandler            ← im/handler.rs
    ↓ 转换为 IncomingMessage
IMGateway.incoming_tx (mpsc 通道)
    ↓
IMGateway.process_incoming_loop()   ← gateway.rs
    ↓ 1. 构建 SessionSource（含 account_id）
    ↓ 2. 查找/创建 AgentSession（确定性哈希 sid）
    ↓ 3. format_im_message() 注入平台上下文
    ↓ 4. AgentRuntime.run_turn()
Agent 处理 → LLM → 工具调用循环
    ↓
IMGateway.reply()                   ← 回复回 IM 平台
```

**Key insight**：每个 IM 渠道必须将平台原生消息转为 `IncomingMessage` 并通过 `incoming_tx` 发送到 IMGateway。

### 6.2 回调处理器模式

文件：[backend/src/im/handler.rs](file:///d:\Project\novaclaw\backend\src\im\handler.rs)

钉钉使用 `CallbackHandler` trait 将消息回调注入 IMGateway：

```rust
/// IMGateway 回调处理器
pub struct IMGatewayCallbackHandler {
    incoming_tx: mpsc::UnboundedSender<IncomingMessage>,
    account_id: String,
    account_name: Option<String>,
}

#[async_trait]
impl crate::dingtalk::handler::CallbackHandler for IMGatewayCallbackHandler {
    async fn on_callback_message(&self, msg: CallbackMessageData, _session_webhook: Option<String>) {
        let incoming_msg = IncomingMessage {
            account_id: self.account_id.clone(),
            account_name: self.account_name.clone(),
            platform: PlatformType::DingTalk,
            // ... 转换字段 ...
        };
        let _ = self.incoming_tx.send(incoming_msg);
    }
}
```

**微信使用不同方式**：长轮询 + 直接转换，详见下文。

---

## 七、新增渠道标准步骤（以飞书为例）

以下步骤基于实际的 Jeeves 代码架构，每一步都提供真实文件路径和代码片段。

### 步骤 1：创建渠道模块目录

```
backend/src/
├── feishu/                     #  新建
│   ├── mod.rs                  #    公共导出
│   ├── config.rs               #    凭据 + 配置类型
│   ├── client.rs               #    HTTP API 客户端封装
│   ├── adapter.rs              #    IMAdapter 实现
│   ├── connection.rs           #    WebSocket/Webhook 连接管理
│   └── event_handler.rs        #    消息转换 + 入站投递
├── im/                         # 已有，核心框架层
├── dingtalk/                   # 已有，WebSocket 模式参考
└── weixin/                     # 已有，长轮询模式参考
```

### 步骤 2：定义凭据和配置类型

文件：`backend/src/im/config.rs`（修改 `IMChannelDetail` 添加飞书字段）

在 `IMChannelDetail` 结构体中添加飞书专用字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IMChannelDetail {
    // ─── 已有字段 ───
    pub webhook: Option<String>,
    pub secret: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,

    // ─── 飞书专用 ───
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub agent_id: Option<String>,
    pub corp_id: Option<String>,
}
```

### 步骤 3：封装 REST API 客户端

文件：`backend/src/feishu/client.rs`

```rust
/// 飞书 API 客户端
pub struct FeishuClient {
    http: reqwest::Client,
    app_id: String,
    app_secret: String,
    token_cache: RwLock<Option<TokenCache>>,
    domain: FeishuDomain,  // Feishu / Lark
}

impl FeishuClient {
    pub fn new(app_id: String, app_secret: String, domain: &str) -> Self { ... }

    /// 获取 tenant_access_token（自动续期）
    pub async fn get_token(&self) -> Result<String, AppError> { ... }

    /// 发送文本消息
    /// POST /open-apis/im/v1/messages?receive_id_type=open_id
    pub async fn send_text(&self, receive_id: &str, receive_id_type: &str, text: &str) -> Result<(), AppError> { ... }

    /// 发送富文本
    pub async fn send_post(&self, receive_id: &str, receive_id_type: &str, content: &str) -> Result<(), AppError> { ... }

    /// 回复消息
    /// POST /open-apis/im/v1/messages/{message_id}/reply
    pub async fn reply_message(&self, message_id: &str, content: &str, msg_type: &str) -> Result<(), AppError> { ... }
}
```

**参考实现**：
- 钉钉 client：[backend/src/dingtalk/message.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\message.rs)（REST 消息发送）
- 微信 client：[backend/src/weixin/client.rs](file:///d:\Project\novaclaw\backend\src\weixin\client.rs)（HTTP 轮询）

### 步骤 4：WebSocket 连接管理

文件：`backend/src/feishu/connection.rs`

飞书支持 WebSocket 和 Webhook 两种模式。WebSocket 模式与 DingTalk 实现基本一致：

**参考 DingTalk 实现**：
- 连接生命周期：[backend/src/dingtalk/connection.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\connection.rs)
- 网关帧处理：[backend/src/dingtalk/gateway.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\gateway.rs)
- 消息帧类型：[backend/src/dingtalk/frames.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\frames.rs)
- Token 管理：[backend/src/dingtalk/credential.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\credential.rs)

```rust
/// WebSocket 连接
/// 1. 获取 tenant_access_token
/// 2. POST /open-apis/ws/v1/app_start 获取 WebSocket URL
/// 3. 连接 WebSocket
/// 4. 接收事件帧、心跳、自动重连
pub async fn start_connection(client: &FeishuClient, event_tx: mpsc::UnboundedSender<FeishuEvent>) -> ... { ... }
```

**与 DingTalk 关键差异**：

| 项目 | DingTalk | 飞书 WebSocket |
|------|----------|---------------|
| 获取连接 URL | `POST /v1.0/gateway/connections/open` | `POST /open-apis/ws/v1/app_start` |
| 心跳间隔 | 60s | 30s |
| 事件类型 | SYSTEM/EVENT/CALLBACK | `im.message.receive_v1` |
| 需要先获取 Token | 否（直连） | 是（tenant_access_token） |

### 步骤 5：事件处理器（消息 → IncomingMessage）

文件：`backend/src/feishu/event_handler.rs`

将飞书原生消息转换为 `IncomingMessage` 并通过 mpsc 投递到 IMGateway：

```rust
pub async fn handle_message_event(
    event: FeishuMessageEvent,
    account_id: &str,
    account_name: Option<String>,
    incoming_tx: &mpsc::UnboundedSender<IncomingMessage>,
) {
    let incoming = IncomingMessage {
        id: event.message.message_id,
        account_id: account_id.to_string(),
        account_name,
        platform: PlatformType::Feishu,
        conversation_id: event.message.chat_id,
        sender_id: Some(event.sender.sender_id.open_id),
        sender_name: None,  // 可通过 contact API 获取
        text: parse_text_content(&event.message.content),
        sender_staff_id: None,
        media_urls: vec![],
        raw: serde_json::to_value(&event).unwrap_or_default(),
        session_webhook: None,  // 飞书无 webhook 回复机制
        conversation_type: match event.message.chat_type.as_str() {
            "p2p" | "private" => ConversationType::Private,
            _ => ConversationType::Group,
        },
        conversation_title: None,
        timestamp: event.message.create_time.parse().unwrap_or(0),
    };
    let _ = incoming_tx.send(incoming);
}
```

**参考两种入站模式**：

| 模式 | 代表平台 | 文件 | 特点 |
|------|---------|------|------|
| **WebSocket + CallbackHandler** | DingTalk | `dingtalk/connection.rs` + `im/handler.rs` | 实时推送，需注册回调处理器 |
| **长轮询 + Adapter 自启动** | WeChat | `weixin/adapter.rs` (`start_polling`) | 定时轮询，适配器内部启动 tokio 任务 |

### 步骤 6：实现 IMAdapter

文件：`backend/src/feishu/adapter.rs`

```rust
pub struct FeishuAdapter {
    client: FeishuClient,
    connected: Arc<AtomicBool>,
}

#[async_trait]
impl IMAdapter for FeishuAdapter {
    fn platform_type(&self) -> PlatformType { PlatformType::Feishu }

    fn is_connected(&self) -> bool { self.connected.load(Ordering::Relaxed) }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: true,
            supports_images: true,
            supports_files: true,
            max_message_length: 4000,
        }
    }

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError> {
        let receive_id_type = match target.conversation_type {
            ConversationType::Private => "open_id",
            ConversationType::Group => "chat_id",
        };
        self.client.send_text(&target.conversation_id, receive_id_type, text).await?;
        Ok(SendResult::ok())
    }

    async fn send_markdown(&self, target: &MessageTarget, title: &str, text: &str) -> Result<SendResult, AppError> {
        let post_content = markdown_to_feishu_post(title, text);
        let receive_id_type = match target.conversation_type {
            ConversationType::Private => "open_id",
            ConversationType::Group => "chat_id",
        };
        self.client.send_post(&target.conversation_id, receive_id_type, &post_content).await?;
        Ok(SendResult::ok())
    }

    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError> {
        self.client.reply_message(&original.id, text, "text").await?;
        Ok(SendResult::ok())
    }

    // 飞书不支持流式回复，使用默认降级
}
```

**参考现有实现**：
- DingTalk Adapter：[backend/src/dingtalk/adapter.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\adapter.rs)（含完整流式 AI Card 支持）
- WeChat Adapter：[backend/src/weixin/adapter.rs](file:///d:\Project\novaclaw\backend\src\weixin\adapter.rs)（含 Markdown 过滤 + 长轮询）

### 步骤 7：模块导出

文件：`backend/src/feishu/mod.rs`

```rust
pub mod adapter;
pub mod client;
pub mod config;
pub mod connection;
pub mod event_handler;

pub use adapter::FeishuAdapter;
pub use client::FeishuClient;
pub use config::FeishuConfig;
```

### 步骤 8：注册到初始化流程

需要修改两处：

#### 8.1 `backend/src/lib.rs` 的 `initialize()` 函数

在渠道匹配分支中添加飞书支持：

```rust
"feishu" => {
    let account_ids = channel.enabled_account_ids();
    if account_ids.is_empty() {
        tracing::warn!("飞书渠道 '{}' 没有有效的账号配置", channel.name);
        continue;
    }

    for account_id in &account_ids {
        let account_cfg = match channel.get_account(account_id) {
            Some(c) => c,
            None => continue,
        };
        if !account_cfg.enabled { continue; }

        tracing::info!("正在连接飞书账号: {} (name={:?})", account_id, account_cfg.name);

        // 1. 创建 FeishuClient
        let fs_client = Arc::new(
            FeishuClient::new(
                account_cfg.credentials.client_id.clone(),
                account_cfg.credentials.client_secret.clone(),
                "feishu",
            )
        );

        // 2. 创建 FeishuAdapter
        let fs_adapter = Arc::new(
            FeishuAdapter::new(fs_client.clone())
        );

        // 3. 启动 WebSocket 事件监听（类似 DingTalk 模式）
        // 注意：这里需要一个 ClientFacade 结构（类似 DingTalkClient）
        // 来管理 WebSocket 连接生命周期和消息回调

        // 4. 注册到 Gateway
        gateway.register(AccountInfo {
            account_id: account_id.clone(),
            platform: PlatformType::Feishu,
            adapter: fs_adapter,
            enabled: true,
            name: account_cfg.name.clone(),
        }).await;

        total_accounts += 1;
        tracing::info!("飞书账号已注册: {} (name={:?})", account_id, account_cfg.name);
    }
}
```

#### 8.2 `backend/src/im/reload.rs` 的 `reload_gateway()` 函数

同样的注册逻辑需要添加到热加载函数中。

### 步骤 9：配置文件示例

`config/im.json` 中添加飞书渠道：

```json
{
  "channels": [
    {
      "id": "feishu_001",
      "name": "飞书助手",
      "channel_type": "feishu",
      "enabled": true,
      "config": {
        "webhook": "https://open.feishu.cn/open-apis/bot/v2/hook/xxx",
        "secret": "webhook-sign-secret"
      },
      "accounts": {
        "bot1": {
          "id": "bot1",
          "name": "飞书机器人",
          "enabled": true,
          "credentials": {
            "clientId": "cli_xxx",
            "clientSecret": "xxx"
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

### 步骤 10：前端配置表单

文件：`src/pages/IMSettings.tsx`

在 `channelTypes` 数组中添加飞书：

```typescript
const channelTypes = [
  { id: 'dingtalk', name: '钉钉', icon: '🔔', color: 'text-blue-400' },
  { id: 'weixin', name: '个人微信', icon: '💬', color: 'text-green-400' },
  { id: 'feishu', name: '飞书', icon: '📮', color: 'text-purple-400' },
]
```

并在渠道配置表单中添加飞书对应的字段渲染（已有 `appId`、`appSecret`、`agentId`、`corpId` 通用字段）。

---

## 八、关键设计决策（与旧版区别）

以下是在修复钉钉和微信 IM 过程中总结的关键设计，**必须在开发新渠道时遵守**：

### 8.1 复合 key 注册（避免多账号冲突）

```
旧设计：HashMap<PlatformType, Arc<dyn IMAdapter>>
         → 多个钉钉机器人只能用同一个 key，后注册覆盖先注册
新设计：HashMap<"{platform}:{account_id}", AccountInfo>
         → "dingtalk:im_001" 和 "dingtalk:im_002" 互不冲突
```

### 8.2 账号 ID 使用渠道唯一 ID

```
旧设计：单账号使用 DEFAULT_ACCOUNT_ID = "default"
         → 所有单账号渠道共享同一个 key
新设计：单账号使用 channel.id（如 "im_1779168106717"）
         → 每个渠道的 account_id 天然唯一
```

### 8.3 入站消息携带 account_name

```
旧设计：IncomingMessage 无 account_name 字段
         → 前端会话列表显示渠道ID，如 "IM 钉钉 私聊(default)"
新设计：IncomingMessage.account_name: Option<String>
         → 前端显示名称，如 "IM 钉钉 私聊(测试)"
```

### 8.4 两阶段注册（适配器 + 消息管道）

```
两种入站模式都需要注册到 Gateway：

DingTalk 模式：
  1. 注册 Adapter → gateway.register(AccountInfo)
  2. 注册 CallbackHandler → dt_client.register_handler(IMGatewayCallbackHandler)
  3. 连接 WebSocket（dt_client 内部管理）

WeChat 模式：
  1. 注册 Adapter → gateway.register(AccountInfo)
  2. 启动长轮询 → adapter.start_polling(incoming_tx)
```

### 8.5 回复路径的三级降级

```
reply() 实现优先级：
  1. session_webhook → 最快的即时回复（钉钉独有）
  2. 平台原生 reply API（如飞书 reply_message、微信 send_text）
  3. send_text/send_markdown 兜底
```

---

## 九、参考实现清单

| 文件 | 内容 | 可作为 |
|------|------|--------|
| [backend/src/dingtalk/adapter.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\adapter.rs) | DingTalk IMAdapter 实现（含流式 AI Card） | 流式渠道参考 |
| [backend/src/weixin/adapter.rs](file:///d:\Project\novaclaw\backend\src\weixin\adapter.rs) | WeChat IMAdapter + 长轮询 + Markdown 过滤 | 非流式/轮询渠道参考 |
| [backend/src/im/handler.rs](file:///d:\Project\novaclaw\backend\src\im\handler.rs) | IMGatewayCallbackHandler | 回调处理器模板 |
| [backend/src/im/registry.rs](file:///d:\Project\novaclaw\backend\src\im\registry.rs) | AccountRegistry 复合 key 注册中心 | 注册 API 参考 |
| [backend/src/im/gateway.rs](file:///d:\Project\novaclaw\backend\src\im\gateway.rs) | IMGateway 消息路由 + Agent 对接 | 入站消息处理流程 |
| [backend/src/im/session.rs](file:///d:\Project\novaclaw\backend\src\im\session.rs) | 会话管理 + 平台上下文注入 | 会话生成逻辑 |
| [backend/src/im/config.rs](file:///d:\Project\novaclaw\backend\src\im\config.rs) | 渠道配置模型（多账号支持） | 配置结构定义 |
| [backend/src/dingtalk/connection.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\connection.rs) | DingTalk WebSocket 连接 | WebSocket 渠道参考 |
| [backend/src/dingtalk/credential.rs](file:///d:\Project\novaclaw\backend\src\dingtalk\credential.rs) | Token 管理 | Token 自动续期参考 |
| [backend/src/weixin/client.rs](file:///d:\Project\novaclaw\backend\src\weixin\client.rs) | WeChat HTTP 客户端（含 iLink 协议头） | HTTP 客户端参考 |
| [backend/src/lib.rs](file:///d:\Project\novaclaw\backend\src\lib.rs) | 初始化流程（多账号迭代注册） | 启动注册参考 |
| [backend/src/im/reload.rs](file:///d:\Project\novaclaw\backend\src\im\reload.rs) | 热加载（配置修改后重新注册所有渠道） | 热加载参考 |

---

## 十、渠道适配开发 Checklist

| # | 步骤 | 文件 |
|---|------|------|
| 1 | 创建 `{platform}/mod.rs` 导出模块 | `backend/src/{platform}/mod.rs` |
| 2 | 定义凭据类型（如有新字段，补充到 `im/config.rs`） | `backend/src/{platform}/config.rs` |
| 3 | 实现 HTTP/WebSocket 客户端 | `backend/src/{platform}/client.rs` |
| 4 | 实现 WebSocket 连接管理（或选择长轮询） | `backend/src/{platform}/connection.rs` |
| 5 | 实现消息转换（原生消息 → IncomingMessage） | `backend/src/{platform}/event_handler.rs` |
| 6 | 实现 IMAdapter trait（send_text / send_markdown / reply） | `backend/src/{platform}/adapter.rs` |
| 7 | 在 `lib.rs` 渠道匹配分支添加注册逻辑 | `backend/src/lib.rs` |
| 8 | 在 `im/reload.rs` 同步添加热加载逻辑 | `backend/src/im/reload.rs` |
| 9 | 前端 `IMSettings.tsx` 添加渠道类型和表单 | `src/pages/IMSettings.tsx` |
| 10 | 编译验证 | `cargo check` |
| 11 | 端到端测试：WebSocket/轮询连接 → 发送消息 → 接收回复 | 手动测试 |

---

## 十一、注意事项

1. **DingTalk 私聊/群聊不同 API**：私聊使用 `send_private_message`，群聊使用 `send_group_message`，reply 方法回退时需正确判断
2. **DingTalk 私聊回复必须用 sender_staff_id**：不能用 conversation_id，否则消息发送到错误目标
3. **WeChat iLink 协议头**：微信 API 需要 `iLink-App-Id` 和 `iLink-App-ClientVersion` 请求头，以及 `client_id`（UUID 格式）和 `base_info` 请求体字段
4. **平台不支持流式**：`start_stream_reply` 返回 `Err`，Gateway 会自动降级为非流式 reply
5. **群聊 @ 过滤**：`should_respond_in_group()` 检查消息中是否 @ 了机器人，避免群聊每条消息都触发 Agent
6. **TOML 配置更新**：前端保存配置后，后台会自动触发热加载（`reload_gateway()`），新渠道即时生效
7. **mod.rs 注册**：新创建的 `{platform}` 模块必须在 `backend/src/lib.rs` 中声明 `pub mod {platform};`