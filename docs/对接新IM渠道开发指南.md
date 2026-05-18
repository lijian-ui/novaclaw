# 对接新 IM 渠道开发指南

> 撰写日期：2026-05-18
> 适用版本：NovaClaw 多渠道 IM 架构 v1
> 参考实现：DingTalk（已有）、Feishu/飞书（本文作为教学案例）

---

## 一、架构概述

NovaClaw 的 IM 系统采用 **Adapter + Registry + Gateway** 三层架构：

```
┌─────────────────────────────────────────────────────────────┐
│                        IMGateway                            │
│  (im/gateway.rs)                                            │
│  消息路由 / Agent 会话管理 / 入站消息循环                      │
├─────────────────────────────────────────────────────────────┤
│                    PlatformRegistry                          │
│  (im/registry.rs)                                           │
│  HashMap<PlatformType, Arc<dyn IMAdapter>>                  │
├────────────┬────────────┬────────────┬──────────────────────┤
│ DingTalk   │  飞书       │  企业微信    │  Slack / ...        │
│ adapter.rs │ adapter.rs │ adapter.rs  │  (未来)              │
├────────────┴────────────┴────────────┴──────────────────────┤
│                     Agent (ReAct 循环)                       │
│  工具: send_im_message / 入站消息触发 Agent.run_turn()       │
└─────────────────────────────────────────────────────────────┘
```

**新增一个渠道只需要做一件事**：实现 `IMAdapter` trait，注册到 `PlatformRegistry`。

---

## 二、核心契约（必须实现）

所有渠道必须实现 `im/adapter.rs` 中的 `IMAdapter` trait：

```rust
#[async_trait]
pub trait IMAdapter: Send + Sync {
    fn platform_type(&self) -> PlatformType;
    fn is_connected(&self) -> bool;
    fn capabilities(&self) -> PlatformCapabilities;

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError>;
    async fn send_markdown(&self, target: &MessageTarget, title: &str, text: &str) -> Result<SendResult, AppError>;
    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError>;
}
```

---

## 三、新增渠道的标准步骤（以飞书为例）

### 步骤 1：创建渠道模块目录

```
backend/src/
├── feishu/                    # ← 新建
│   ├── mod.rs                 #   公共导出
│   ├── config.rs              #   凭据 + 配置管理
│   ├── client.rs              #   HTTP API 客户端封装
│   ├── adapter.rs             #   IMAdapter 实现
│   ├── connection.rs          #   连接生命周期（WebSocket / Webhook）
│   ├── frames.rs              #   消息类型定义
│   └── event_handler.rs       #   事件处理器（消息接收）
├── im/                        # 已有，无需修改
└── dingtalk/                  # 已有，参考用
```

### 步骤 2：定义凭据和配置（`feishu/config.rs`）

飞书自建应用需要以下凭据：

```rust
/// 飞书渠道配置（用户通过前端 IMSettings 页面配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuConfig {
    /// App ID（飞书开放平台获取）
    pub app_id: String,
    /// App Secret
    pub app_secret: String,
    /// 事件加密密钥（Webhook 模式必填）
    pub encrypt_key: Option<String>,
    /// 事件验证令牌（Webhook 模式必填）
    pub verification_token: Option<String>,
    /// 连接模式：websocket（默认）或 webhook
    /// webhook 需要公网可访问的 HTTP 端点
    #[serde(default = "default_connection_mode")]
    pub connection_mode: String,
    /// 海外版域名（lark）或国内（feishu）
    #[serde(default = "default_domain")]
    pub domain: String,
}

fn default_connection_mode() -> String { "websocket".into() }
fn default_domain() -> String { "feishu".into() }
```

**用户前端配置入口**：在 `IMSettings.tsx` 的 `channelTypes` 数组中添加飞书渠道：

```typescript
const channelTypes = [
  { id: 'dingtalk', name: '钉钉', icon: '🔔', color: 'text-blue-400' },
  { id: 'feishu', name: '飞书', icon: '📮', color: 'text-green-400' },   // ← 已有
  // { id: 'feishu', name: '飞书', ... }  // 如已存在则无需重复添加
]
```

**IM 配置持久化**：不需要额外代码，现有 `im/config.rs` 配置模型已包含飞书需要的字段（`app_id`, `app_secret`, `webhook`, `secret`）：

```json
{
  "channels": [
    {
      "id": "feishu",
      "name": "飞书",
      "enabled": true,
      "config": {
        "app_id": "cli_xxx",
        "app_secret": "xxx",
        "encrypt_key": "xxx",
        "verification_token": "xxx",
        "webhook": "https://open.feishu.cn/open-apis/bot/v2/hook/xxx",
        "secret": "webhook-sign-secret"
      }
    }
  ]
}
```

> **注意**：如果 `IMChannelDetail` 结构体缺少飞书所需的字段，需要在 `im/config.rs` 中补充。

### 步骤 3：封装飞书 REST API 客户端（`feishu/client.rs`）

飞书使用 `@larksuiteoapi/node-sdk`，但在 Rust 中需要自行实现或使用社区 SDK。核心 API：

```rust
/// 飞书 API 客户端
pub struct FeishuClient {
    http: reqwest::Client,
    app_id: String,
    app_secret: String,
    /// 缓存的 tenant_access_token
    token_cache: RwLock<Option<TokenCache>>,
    domain: FeishuDomain,
}

enum FeishuDomain {
    Feishu,  // https://open.feishu.cn
    Lark,    // https://open.larksuite.com
}

impl FeishuClient {
    /// 获取 tenant_access_token（自动续期，参考 dingtalk/credential.rs）
    pub async fn get_token(&self) -> Result<String, AppError> { ... }

    // ─── 消息发送 API ───

    /// 发送文本消息（POST /open-apis/im/v1/messages）
    pub async fn send_text(
        &self,
        receive_id: &str,
        receive_id_type: &str,  // "open_id" | "chat_id"
        text: &str,
    ) -> Result<(), AppError> { ... }

    /// 发送富文本/Markdown 消息
    pub async fn send_post(
        &self,
        receive_id: &str,
        receive_id_type: &str,
        content: &str,
    ) -> Result<(), AppError> { ... }

    /// 回复消息（POST /open-apis/im/v1/messages/{message_id}/reply）
    pub async fn reply_message(
        &self,
        message_id: &str,
        content: &str,
        msg_type: &str,  // "text" | "post"
    ) -> Result<(), AppError> { ... }

    // ─── 媒体上传 API ───

    /// 上传图片（POST /open-apis/im/v1/images）
    pub async fn upload_image(&self, data: Vec<u8>, image_type: &str) -> Result<String, AppError> { ... }

    /// 上传文件（POST /open-apis/im/v1/files）
    pub async fn upload_file(&self, data: Vec<u8>, file_name: &str) -> Result<String, AppError> { ... }
}
```

**飞书 REST API 端点对照**：

| 操作 | 方法 | URL | 参考 dingtalk |
|------|------|-----|-------------|
| 获取 token | POST | `/open-apis/auth/v3/tenant_access_token/internal` | `credential.rs` |
| 发送消息 | POST | `/open-apis/im/v1/messages?receive_id_type=open_id` | `message.rs` |
| 回复消息 | POST | `/open-apis/im/v1/messages/{message_id}/reply` | (新) |
| 上传图片 | POST | `/open-apis/im/v1/images` | (新) |
| 上传文件 | POST | `/open-apis/im/v1/files` | (新) |
| 获取消息 | GET | `/open-apis/im/v1/messages/{message_id}` | (新) |

**请求格式**：

- 获取 token：
  ```json
  POST https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal
  { "app_id": "cli_xxx", "app_secret": "xxx" }
  ```
  响应：
  ```json
  { "code": 0, "msg": "success", "tenant_access_token": "xxx", "expire": 7200 }
  ```

- 发送文本消息：
  ```json
  POST https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=open_id
  Authorization: Bearer {tenant_access_token}
  {
    "receive_id": "ou_xxx",
    "msg_type": "text",
    "content": "{\"text\":\"hello\"}"
  }
  ```
  响应：
  ```json
  { "code": 0, "msg": "success", "data": { "message_id": "om_xxx" } }
  ```

- 发送富文本消息（`msg_type: "post"`）：
  ```json
  {
    "receive_id": "ou_xxx",
    "msg_type": "post",
    "content": "{\"zh_cn\":{\"title\":\"\",\"content\":[[{\"tag\":\"text\",\"text\":\"hello\"}]]}}"
  }
  ```

- 回复消息：
  ```json
  POST https://open.feishu.cn/open-apis/im/v1/messages/{message_id}/reply
  Authorization: Bearer {tenant_access_token}
  { "content": "{\"text\":\"回复内容\"}", "msg_type": "text" }
  ```

### 步骤 4：WebSocket 连接管理（`feishu/connection.rs`）

飞书支持 WebSocket 和 Webhook 两种事件接收模式。

#### WebSocket 模式（推荐，与钉钉一致）

参考 `dingtalk/connection.rs`，模式几乎相同：

```rust
/// 飞书 WebSocket 连接配置
pub struct FeishuConnectionConfig {
    pub app_id: String,
    pub app_secret: String,
    pub auto_reconnect: bool,
    pub reconnect_interval_secs: u64,
    pub ping_interval_secs: u64,  // 飞书默认 30s
    pub ping_timeout_secs: u64,   // 飞书默认 3s
}

/// 启动飞书 WebSocket 连接
/// 
/// 与 dingtalk 的流式模式非常相似：
///   1. 获取 tenant_access_token
///   2. POST /open-apis/ws/v1/app_start 获取 WebSocket URL
///   3. 连接 WebSocket
///   4. 接收事件帧、发送心跳、自动重连
pub async fn start_connection(
    client: FeishuClient,
    config: FeishuConnectionConfig,
    event_tx: mpsc::UnboundedSender<FeishuEvent>,
) -> FeishuConnection { ... }
```

**与 DingTalk 的关键差异**：

| 差异点 | DingTalk | 飞书 WebSocket |
|--------|----------|---------------|
| 获取连接 URL | `POST /v1.0/gateway/connections/open` | `POST /open-apis/ws/v1/app_start` |
| 认证方式 | clientId + clientSecret 直接传 | 需要先获取 `tenant_access_token` |
| 心跳间隔 | 60 秒 | 30 秒 |
| 帧格式 | JSON 帧 | JSON 帧 |
| 事件类型 | SYSTEM / EVENT / CALLBACK | `im.message.receive_v1` 等事件名 |
| ACK 机制 | 发送 ACK 帧确认 | 飞书客户端自动 ACK（框架处理） |

#### Webhook 模式（备选）

当用户无法使用 WebSocket（如内部网络限制）时，飞书支持 Webhook 回调。需要：

1. 提供一个公网可访问的 HTTP 端点（如 `POST /api/im/feishu/webhook`）
2. 在飞书开放平台配置事件订阅 URL
3. 验证签名（`SHA256(timestamp + nonce + encryptKey + body)`）
4. 响应 URL Challenge

```rust
/// 飞书 Webhook 验证和事件处理
pub async fn handle_webhook(
    headers: HeaderMap,
    body: Bytes,
    encrypt_key: &str,
    verification_token: &str,
) -> Result<Json<Value>, AppError> {
    // 1. URL Challenge：首次配置时飞书会发送 challenge
    //    需要原样返回 challenge 值
    if let Some(challenge) = body.get("challenge") {
        return Ok(json!({ "challenge": challenge }));
    }
    
    // 2. 验证签名
    let timestamp = headers.get("x-lark-request-timestamp");
    let nonce = headers.get("x-lark-request-nonce");
    let signature = headers.get("x-lark-signature");
    verify_signature(timestamp, nonce, body, encrypt_key, signature)?;
    
    // 3. 解密事件数据（如果配置了 encrypt_key）
    let event = decrypt_event(body, encrypt_key)?;
    
    // 4. 路由事件到处理器
    handle_event(event).await;
    
    Ok(json!({}))
}
```

**模式选择**：建议优先使用 WebSocket 模式，因为它不需要公网 IP，连接更稳定。

### 步骤 5：事件处理器（`feishu/event_handler.rs`）

消息接收后，转换为统一的 `IncomingMessage` 并送入 IMGateway：

```rust
use crate::im::types::{IncomingMessage, ConversationType, PlatformType};

/// 处理飞书消息事件
pub async fn handle_message_event(
    event: FeishuMessageEvent,
    app_id: &str,
    incoming_tx: &mpsc::UnboundedSender<IncomingMessage>,
) {
    let chat_type = match event.message.chat_type.as_str() {
        "p2p" | "private" => ConversationType::Private,
        "group" | "topic_group" => ConversationType::Group,
        _ => ConversationType::Private,
    };

    let chat_id = event.message.chat_id.clone();
    let sender_id = event.sender.sender_id.open_id.clone();
    let message_id = event.message.message_id.clone();
    let msg_type = event.message.message_type.as_str();
    let content = event.message.content; // JSON 字符串，需按 msg_type 解析

    // 解析消息内容
    let text = match msg_type {
        "text" => parse_text_content(&content),       // {"text":"hello"}
        "post" => parse_post_content(&content),        // 富文本转纯文本
        "image" => "[图片]".to_string(),                // 图片消息
        "file" => "[文件]".to_string(),                 // 文件消息
        _ => "[不支持的消息类型]".to_string(),
    };

    let incoming = IncomingMessage {
        id: message_id,
        platform: PlatformType::Feishu,
        conversation_id: chat_id,
        sender_id: Some(sender_id),
        sender_name: None, // 可通过 GET /open-apis/contact/v3/users/{id} 获取
        text,
        media_urls: vec![], // 图片/文件需额外下载
        raw: serde_json::to_value(&event).unwrap_or_default(),
        session_webhook: None, // 飞书没有 webhook 回复机制
        conversation_type: chat_type,
        conversation_title: None, // 可通过 GET /open-apis/im/v1/chats/{id} 获取
        timestamp: event.message.create_time.parse().unwrap_or(0),
    };

    if let Err(e) = incoming_tx.send(incoming) {
        tracing::error!("飞书消息入站失败: {}", e);
    }
}

/// 解析飞书 text 消息内容
fn parse_text_content(content: &str) -> String {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.get("text").and_then(|t| t.as_str().map(|s| s.to_string())))
        .unwrap_or_else(|| content.to_string())
}

/// 解析飞书 post（富文本）消息内容为纯文本
fn parse_post_content(content: &str) -> String {
    // 飞书 post 格式复杂，需递归提取所有 text 标签
    // 简化实现：尝试解析并提取文本
    content.to_string() // 实际需要 JSON 解析后遍历 rich text 结构
}
```

### 步骤 6：实现 IMAdapter（`feishu/adapter.rs`）

这是将飞书 API 接入 NovaClaw IM 抽象层的核心适配器：

```rust
use crate::feishu::client::FeishuClient;
use crate::feishu::config::FeishuConfig;
use crate::im::adapter::IMAdapter;
use crate::im::types::*;

/// 飞书 IM 适配器
pub struct FeishuAdapter {
    client: FeishuClient,
    config: FeishuConfig,
    connected: Arc<AtomicBool>,
}

impl FeishuAdapter {
    pub fn new(config: FeishuConfig) -> Self {
        let client = FeishuClient::new(
            config.app_id.clone(),
            config.app_secret.clone(),
            &config.domain,
        );
        Self {
            client,
            config,
            connected: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl IMAdapter for FeishuAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Feishu
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: true,
            supports_images: true,
            supports_files: true,
            max_message_length: 4000,
        }
    }

    async fn send_text(
        &self,
        target: &MessageTarget,
        text: &str,
    ) -> Result<SendResult, AppError> {
        let receive_id_type = match target.conversation_type {
            ConversationType::Private => "open_id",
            ConversationType::Group => "chat_id",
        };
        self.client
            .send_text(&target.conversation_id, receive_id_type, text)
            .await?;
        Ok(SendResult::ok())
    }

    async fn send_markdown(
        &self,
        target: &MessageTarget,
        title: &str,
        text: &str,
    ) -> Result<SendResult, AppError> {
        // 飞书用 post 消息类型发 Markdown
        let post_content = markdown_to_feishu_post(title, text);
        let receive_id_type = match target.conversation_type {
            ConversationType::Private => "open_id",
            ConversationType::Group => "chat_id",
        };
        self.client
            .send_post(&target.conversation_id, receive_id_type, &post_content)
            .await?;
        Ok(SendResult::ok())
    }

    async fn reply(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError> {
        // 飞书支持直接回复消息（im.message.reply API）
        self.client.reply_message(&original.id, text, "text").await?;
        Ok(SendResult::ok())
    }
}

/// Markdown → 飞书 Post 格式转换（简化版）
fn markdown_to_feishu_post(title: &str, md_text: &str) -> String {
    // 将 Markdown 转换为飞书 Post JSON 格式
    // 飞书 Post 结构：{ "zh_cn": { "title": "...", "content": [[...]] } }
    // 每个段落是一个数组，段落中的块是 { "tag": "text", "text": "..." }
    // 或 { "tag": "a", "text": "...", "href": "..." }
    // TODO: 实现完整的 Markdown → Post 转换
    serde_json::json!({
        "zh_cn": {
            "title": title,
            "content": [[
                { "tag": "text", "text": md_text }
            ]]
        }
    }).to_string()
}
```

### 步骤 7：模块导出（`feishu/mod.rs`）

```rust
pub mod adapter;
pub mod client;
pub mod config;
pub mod connection;
pub mod event_handler;
pub mod frames;

pub use adapter::FeishuAdapter;
pub use client::FeishuClient;
pub use config::FeishuConfig;
```

### 步骤 8：注册到 lib.rs 初始化流程

在 `lib.rs` 的 `initialize()` 函数中添加飞书支持：

```rust
// 在 IM Gateway 初始化部分，遍历 im.json 的 channels
match channel.id.as_str() {
    "dingtalk" => {
        // ... 已有的钉钉逻辑 ...
    }
    "feishu" => {
        if let (Some(app_id), Some(app_secret)) = (
            channel.config.app_id.as_ref(),
            channel.config.app_secret.as_ref(),
        ) {
            tracing::info!("正在启动飞书集成...");
            let feishu_config = feishu::config::FeishuConfig {
                app_id: app_id.clone(),
                app_secret: app_secret.clone(),
                encrypt_key: channel.config.secret.clone(),
                verification_token: None,
                connection_mode: "websocket".to_string(),
                domain: "feishu".to_string(),
            };
            
            // 创建适配器
            let adapter = Arc::new(feishu::FeishuAdapter::new(feishu_config));
            
            // 如果使用 WebSocket 流式模式，启动连接并注册事件管道
            // (类似于 dingtalk 的 DingTalkClient 模式，需要额外实现)
            
            gateway.register(adapter).await;
            tracing::info!("飞书集成已注册到 IMGateway");
        }
    }
    _ => {
        tracing::warn!("不支持的 IM 渠道类型: {}", channel.id);
    }
}
```

### 步骤 9：注册 API 路由（可选）

如果需要通过 HTTP 接收飞书 Webhook 事件，添加路由：

```rust
// src/server/routes/im.rs 中新增端点
.route("/im/feishu/webhook", post(handle_feishu_webhook))
```

同时需要更新 `src/server/routes/mod.rs` 中的路由合并。

### 步骤 10：前端配置字段同步

在 `src/pages/IMSettings.tsx` 中，飞书渠道的表单字段已存在（`appId`, `appSecret`, `agentId`, `corpId`），无需修改。但需确保 `IMChannelDetail` 类型包含 `encrypt_key`, `verification_token`, `connection_mode`, `domain` 等字段。

---

## 四、文件清单（完整的新渠道项目结构）

```
backend/src/feishu/
├── mod.rs                          # 模块导出
├── config.rs                       # FeishuConfig 结构体
├── client.rs                       # FeishuClient REST API 封装
│                                   #   get_token() / send_text() / send_post()
│                                   #   reply_message() / upload_image() / upload_file()
├── frames.rs                       # 飞书消息/事件类型（可选）
│                                   #   FeishuMessageEvent / FeishuTokenResponse
├── connection.rs                   # WebSocket 连接生命周期
│                                   #   start_connection() / ws_read_loop()
│                                   #   keepalive() / auto_reconnect()
├── event_handler.rs                # 事件处理器
│                                   #   handle_message_event() → IncomingMessage
│                                   #   parse_text_content() / parse_post_content()
│                                   #   群聊 @ 检查
└── adapter.rs                      # FeishuAdapter implements IMAdapter
                                    #   send_text / send_markdown / reply

backend/src/im/config.rs            # (修改) IMChannelDetail 补充飞书字段
```

**与 DingTalk 的差异总结**：

| 维度 | DingTalk | 飞书 |
|------|----------|------|
| 认证 | clientId + clientSecret | appId + appSecret → tenant_access_token |
| 消息接收 | 流式 WebSocket（Stream 模式） | WebSocket（官方 SDK）或 Webhook |
| 消息发送 | 私聊/群聊分开 API | 统一 `POST /messages?receive_id_type=...` |
| 回复方式 | sessionWebhook 优先 | `POST /messages/{id}/reply` |
| Markdown | 直接传 content 字段 | 需转换为 Post 富文本格式 |
| 心跳 | 60s 发 `{"code":200,"message":"ping"}` | 30s，SDK 自动处理 |
| 媒体发送 | 先上传到 `oapi.dingtalk.com` | 先上传到 `open.feishu.cn/open-apis/im/v1/images` |

---

## 五、IMChannelDetail 扩展

当前 `im/config.rs` 中的 `IMChannelDetail` 结构需要补充飞书特有的字段：

```rust
/// 渠道具体配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMChannelDetail {
    // ─── Webhook 模式（通用） ───
    pub webhook: Option<String>,
    pub secret: Option<String>,

    // ─── Stream 模式（钉钉/飞书通用） ───
    pub client_id: Option<String>,
    pub client_secret: Option<String>,

    // ─── 飞书专用 ───
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub agent_id: Option<String>,
    pub corp_id: Option<String>,
    pub encrypt_key: Option<String>,         // ← 新增
    pub verification_token: Option<String>,   // ← 新增
    pub connection_mode: Option<String>,      // ← 新增: "websocket" / "webhook"
    pub domain: Option<String>,               // ← 新增: "feishu" / "lark"
}
```

---

## 六、常见问题

### Q1：新增渠道是否需要修改 `im/` 模块的代码？

否。`im/` 模块完全解耦，新增渠道只需：
1. 创建渠道目录（如 `feishu/`）
2. 实现 `IMAdapter` trait
3. 在 `lib.rs` 的初始化循环中注册

### Q2：IM 配置如何与前端同步？

前端通过 `GET/POST /api/config/im_channels` 读写 `config/im.json`。新渠道的配置字段只需在 `IMChannelDetail` 结构体中定义即可自动序列化。

### Q3：WebSocket 和 Webhook 如何选择？

| 条件 | 推荐模式 |
|------|---------|
| 有公网 IP 或内网穿透 | WebSocket（简单、稳定） |
| IM 平台仅支持 Webhook | Webhook（如企业微信回调） |
| 用户网络限制 WebSocket | Webhook（需公网可达） |
| 需要低延迟 | WebSocket |

### Q4：渠道实现的质量检查清单

- [ ] `IMAdapter` 所有方法已实现，未使用 `todo!()`
- [ ] `send_text` 能发送纯文本消息
- [ ] `send_markdown` 能发送 Markdown 渲染消息
- [ ] `reply` 能正确回复原始消息（使用平台的 reply API）
- [ ] `is_connected` 返回正确的连接状态
- [ ] `capabilities` 如实声明了平台能力
- [ ] WebSocket 连接有自动重连机制
- [ ] 心跳/保活机制已实现
- [ ] 群聊消息会检查 @ 提及
- [ ] 错误不会导致整个 gateway 崩溃（每个消息独立 try 块）
- [ ] 配置通过 `config/im.json` 读写
- [ ] 前端 IMSettings 页面能显示和编辑该渠道的配置
