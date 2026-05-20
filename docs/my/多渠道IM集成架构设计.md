# NovaClaw 多渠道 IM 集成架构设计

> 撰写日期：2026-05-18
> 技术栈：Rust + Axum + tokio-tungstenite
> 参考项目：OpenClaw（TS，插件式适配器）、Hermes Agent（Python，Adapter+Registry+Runner）

---

## 一、架构总览

采用 **Adapter + Registry + Runner** 三层模式（与 Hermes Agent 一致），利用 Rust trait 实现运行时多态。

```
┌─────────────────────────────────────────────────────────────────┐
│                        IMGateway                                │
│  (im/gateway.rs)                                                │
│  消息路由、会话管理、Agent 对接                                     │
├─────────────────────────────────────────────────────────────────┤
│                     PlatformRegistry                             │
│  (im/registry.rs)                                               │
│  HashMap<PlatformType, Arc<dyn IMAdapter>>                      │
├─────────┬──────────┬──────────┬──────────┬──────────┬──────────┤
│ DingTalk│ 企业微信   │  Slack   │ 飞书     │Telegram  │  ...未来  │
│ adapter │ adapter  │ adapter  │ adapter  │ adapter  │          │
│ dingtalk│ wecom/   │ slack/   │ feishu/  │ telegram/│          │
│ /       │          │          │          │          │          │
└─────────┴──────────┴──────────┴──────────┴──────────┴──────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         AI Agent                                 │
│                        (ReAct 循环)                               │
│        调用 send_im_message 工具 → IMGateway 路由                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 二、目录结构

```
backend/src/
├── im/                            # ← 统一 IM 抽象层（新增）
│   ├── mod.rs                     #   模块导出 + 初始化
│   ├── types.rs                   #   跨平台消息类型定义
│   ├── adapter.rs                 #   IMAdapter trait（核心契约）
│   ├── registry.rs                #   PlatformRegistry（注册中心）
│   ├── gateway.rs                 #   IMGateway（消息路由 + Agent 对接）
│   └── session.rs                 #   跨平台会话管理
├── dingtalk/                      # 已有，保持不变
│   ├── mod.rs                     # DingTalkClient 公共外观
│   ├── connection.rs              # WebSocket 生命周期
│   ├── credential.rs              # Token 管理
│   ├── frames.rs                  # 消息帧类型
│   ├── gateway.rs                 # 网关连接
│   ├── handler.rs                 # 消息处理器
│   └── message.rs                 # 消息发送
├── tools/
│   └── builtin.rs                 # 新增 send_im_message 工具
├── lib.rs                         # initialize() 启动 IMGateway
├── ...
```

## 三、核心数据模型（`im/types.rs`）

### 3.1 平台类型

```rust
/// 平台类型枚举
/// 使用枚举 + Custom 变体，兼顾类型安全和可扩展性
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformType {
    /// 钉钉
    DingTalk,
    /// 企业微信
    WeChatWork,
    /// 飞书
    Feishu,
    /// Slack
    Slack,
    /// Discord
    Discord,
    /// Telegram
    Telegram,
    /// 自定义平台（通过 env 配置的热插拔平台）
    Custom(String),
}

impl PlatformType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlatformType::DingTalk => "dingtalk",
            PlatformType::WeChatWork => "wecom",
            PlatformType::Feishu => "feishu",
            PlatformType::Slack => "slack",
            PlatformType::Discord => "discord",
            PlatformType::Telegram => "telegram",
            PlatformType::Custom(s) => s.as_str(), // 注意：这里返回引用生命周期不对，需调整
        }
    }
}
```

> **设计参考**：Hermes Agent 的 `Platform` 枚举 + `_missing_()` 动态创建机制。Rust 更简单，用 `Custom(String)` 变体即可。

### 3.2 消息类型

```rust
/// 标准化入站消息
/// 由各个平台的适配器将平台原生消息转换为此格式
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// 消息 ID（平台原生）
    pub id: String,
    /// 来源平台
    pub platform: PlatformType,
    /// 会话 ID（私聊=用户ID，群聊=群ID）
    pub conversation_id: String,
    /// 发送者 ID
    pub sender_id: Option<String>,
    /// 发送者昵称
    pub sender_name: Option<String>,
    /// 消息文本内容
    pub text: String,
    /// 媒体资源 URL 列表
    pub media_urls: Vec<String>,
    /// 消息内容的原始 JSON（调试/转发用）
    pub raw: serde_json::Value,
    /// 会话 Webhook URL（钉钉独有，用于快速回复）
    pub session_webhook: Option<String>,
    /// 会话类型（私聊/群聊）
    pub conversation_type: ConversationType,
    /// 群聊名称
    pub conversation_title: Option<String>,
    /// 消息时间戳（毫秒）
    pub timestamp: i64,
}

/// 消息发送目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTarget {
    /// 目标平台
    pub platform: PlatformType,
    /// 会话 ID
    pub conversation_id: String,
}

/// 会话类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationType {
    Private,
    Group,
}

/// 消息内容类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContentType {
    Text,
    Image,
    File,
    Audio,
    Video,
    RichText,
    System,
}

/// 发送结果
#[derive(Debug, Clone)]
pub struct SendResult {
    pub success: bool,
    pub message_id: Option<String>,
    pub error: Option<String>,
}

/// 平台能力声明
/// 参考 OpenClaw 的 ChannelCapabilities
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// 是否支持 Markdown 渲染
    pub supports_markdown: bool,
    /// 是否支持发送图片
    pub supports_images: bool,
    /// 是否支持发送文件
    pub supports_files: bool,
    /// 是否支持富文本
    pub supports_rich_text: bool,
    /// 单条消息最大长度
    pub max_message_length: usize,
}
```

### 3.3 跨平台会话标识

```rust
/// 跨平台会话来源标识
/// 参考 Hermes Agent 的 SessionSource 设计
/// 用来统一所有平台的会话查找
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SessionSource {
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
}
```

---

## 四、核心 Trait（`im/adapter.rs`）

```rust
use async_trait::async_trait;

/// IM 平台适配器契约
///
/// 每个 IM 平台实现此 trait，IMGateway 通过 trait object 统一操作。
/// 
/// 参考：Hermes Agent 的 BasePlatformAdapter、OpenClaw 的 ChannelPlugin
#[async_trait]
pub trait IMAdapter: Send + Sync {
    /// 返回平台类型标识
    fn platform_type(&self) -> PlatformType;

    /// 适配器是否已连接就绪
    fn is_connected(&self) -> bool;

    /// 返回平台能力声明
    fn capabilities(&self) -> PlatformCapabilities;

    /// 发送文本消息到指定目标
    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError>;

    /// 发送 Markdown 消息
    async fn send_markdown(
        &self,
        target: &MessageTarget,
        title: &str,
        text: &str,
    ) -> Result<SendResult, AppError>;

    /// 回复原始消息（利用平台的回复/Webhook 机制）
    async fn reply(
        &self,
        original: &IncomingMessage,
        text: &str,
    ) -> Result<SendResult, AppError>;
}
```

---

## 五、注册中心（`im/registry.rs`）

```rust
/// 平台注册中心
///
/// 内部维护 HashMap<PlatformType, Arc<dyn IMAdapter>>。
/// 支持注册、查找、状态检查。
///
/// 参考：Hermes Agent 的 PlatformRegistry（模块级单例）
pub struct PlatformRegistry {
    adapters: RwLock<HashMap<PlatformType, Arc<dyn IMAdapter>>>,
}

impl PlatformRegistry {
    pub fn new() -> Self;

    /// 注册平台适配器（后注册同类型覆盖先注册）
    pub async fn register(&self, adapter: Arc<dyn IMAdapter>);

    /// 获取指定平台的适配器
    pub fn get(&self, platform: &PlatformType) -> Option<Arc<dyn IMAdapter>>;

    /// 获取所有已注册的平台类型列表
    pub fn platforms(&self) -> Vec<PlatformType>;

    /// 检查指定平台是否已连接
    pub fn is_connected(&self, platform: &PlatformType) -> bool;
}
```

### 注册中心与 Gateway 的关系

```
                   ┌─────────────────────────┐
                   │       IMGateway          │
                   │  ┌───────────────────┐   │
                   │  │ PlatformRegistry  │   │
                   │  │   HashMap<...>    │   │
                   │  └───────────────────┘   │
                   │  ┌───────────────────┐   │
                   │  │  mpsc 入站通道     │   │
                   │  │ IncomingMessage    │   │
                   │  └───────────────────┘   │
                   │  ┌───────────────────┐   │
                   │  │  Agent 对接        │   │
                   │  │  (工具拦截 + 路由)  │   │
                   │  └───────────────────┘   │
                   └─────────────────────────┘
```

---

## 六、消息流

### 6.1 入站（IM → Agent）

```
IM 平台（如钉钉服务器）
    │
    ▼ WebSocket 消息
DingTalkConnection.ws_read_loop()
    │ 解析为 DownStreamMessage
    ▼
DingTalkHandlerRegistry.dispatch_callback()
    │ 包装为 IncomingMessage
    ▼ mpsc::send()
IMGateway.incoming_tx
    │
    ▼ mpsc::recv()
IMGateway.process_incoming()
    │ 1. 构造 SessionSource
    │ 2. 查找/创建 Agent 会话
    │ 3. 注入系统提示（标识平台来源）
    │ 4. 调用 AgentRuntime.run_turn()
    ▼
Agent 处理 → LLM → 工具调用循环
```

### 6.2 出站（Agent → IM）

```
Agent 生成回复 / 调用 send_im_message 工具
    │
    ▼ 参数: { platform, conversation_id, text }
tools::registry 执行 send_im_message
    │ 调用 IMGateway.send_message()
    ▼
IMGateway 查 registry 找到对应适配器
    │
    ▼ adapter.send_text(target, text)
DingTalkAdapter.send_text()
    │ 调用内部 DingTalkClient.send_private_message()
    ▼
钉钉 REST API → 用户看到回复
```

### 6.3 出站全过程（无工具调用，Agent 直接回复）

```
Agent 生成纯文本回复
    │
    ▼ AgentRuntime 返回 AgentResult { content, ... }
IMGateway (或上游调用方) 获取回复
    │
    ▼ 根据 source 记录找到原始平台
    adapter.reply(&original_message, content)
    ▼
用户收到回复
```

> **注意**：当 Agent 在 ReAct 循环中直接生成最终回复文本（没有工具调用）时，回复需要由发起调用的一方（IMGateway 或 HTTP handler）负责发送回 IM 平台。当 Agent 使用 `send_im_message` 工具时，工具自身负责发送。

---

## 七、DingTalk 适配器（已有模块的适配）

现有的 `dingtalk/` 模块**不需要修改**，只需新增一个 `DingTalkAdapter` 实现 `IMAdapter` trait，包装内部调用：

```rust
// 此代码放在 backend/src/dingtalk/adapter.rs（新增）
// 或在 im/ 模块中引用 dingtalk crate

pub struct DingTalkAdapter {
    client: Arc<DingTalkClient>,
}

#[async_trait]
impl IMAdapter for DingTalkAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::DingTalk
    }

    fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: true,
            supports_images: false,   // 钉钉机器人图片需先上传 media_id
            supports_files: false,
            supports_rich_text: false,
            max_message_length: 20000,
        }
    }

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError> {
        // 判断私聊/群聊：conversation_id 格式不同
        // 注：钉钉的私聊和群聊需要不同的 API 调用
        // 这里需要根据格式判断，或在 MessageTarget 中增加字段
        todo!("根据 conversation_id 格式调用 send_private 或 send_group")
    }

    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError> {
        if let Some(webhook) = &original.session_webhook {
            // 优先使用 Webhook 回复（钉钉独有优化）
            self.client.reply_via_webhook(webhook, text).await?;
        } else {
            // 兜底：使用 send_text
            let target = MessageTarget {
                platform: PlatformType::DingTalk,
                conversation_id: original.conversation_id.clone(),
            };
            self.send_text(&target, text).await?;
        }
        Ok(SendResult { success: true, message_id: None, error: None })
    }

    // ...
}
```

---

## 八、Agent 工具集成

### 新增工具：`send_im_message`

```rust
/// send_im_message 工具定义
///
/// Agent 通过此工具主动向任何已注册的 IM 平台发送消息。
/// 参数举例：{ "platform": "dingtalk", "conversation_id": "xxx", "text": "你好" }
{
    name: "send_im_message",
    description: "向已连接的 IM 平台（钉钉/微信/Slack 等）发送消息",
    parameters: {
        type: "object",
        properties: {
            platform: { type: "string", description: "目标平台: dingtalk/wecom/slack/..." },
            conversation_id: { type: "string", description: "会话 ID（私聊ID或群聊ID）" },
            text: { type: "string", description: "消息内容，支持 Markdown" },
        },
        required: ["platform", "conversation_id", "text"],
    },
}
```

### 工具执行逻辑

```rust
async fn execute_send_im_message(
    platform: String,
    conversation_id: String,
    text: String,
) -> ToolResult {
    let platform_type = PlatformType::from_str(&platform)?;
    let gateway = IM_GATEWAY.get().ok_or("IMGateway 未初始化")?;
    let target = MessageTarget { platform: platform_type, conversation_id };
    let result = gateway.send(&target, &text).await?;
    ToolResult::Success(format!("消息已发送到 {}", platform))
}
```

---

## 九、初始化流程（`lib.rs`）

```rust
/// 全局 IMGateway 实例
pub static IM_GATEWAY: Lazy<Arc<RwLock<Option<Arc<im::gateway::IMGateway>>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(None)));

pub async fn initialize() {
    // ... 现有初始化代码 ...

    // 启动 IM Gateway
    let gateway = im::gateway::IMGateway::new();
    
    // 注册 DingTalk（如果配置了凭据）
    if let (Some(cid), Some(cs)) = (dingtalk_cid, dingtalk_cs) {
        let dt_client = Arc::new(dingtalk::DingTalkClient::new(cid, cs).await);
        let dt_adapter = Arc::new(dingtalk::DingTalkAdapter::new(dt_client));
        gateway.register(dt_adapter).await;
    }

    // 注册其他平台（未来扩展）
    // if let Some(config) = wecom_config {
    //     gateway.register(Arc::new(wecom::WeChatWorkAdapter::new(config))).await;
    // }

    // 保存到全局
    {
        let mut g = IM_GATEWAY.write().await;
        *g = Some(Arc::new(gateway));
    }
}
```

---

## 十、实施步骤

| 步骤 | 文件 | 内容 |
|------|------|------|
| 1 | `im/types.rs` | 定义 PlatformType、IncomingMessage、MessageTarget、SendResult、PlatformCapabilities |
| 2 | `im/adapter.rs` | 定义 IMAdapter trait |
| 3 | `im/registry.rs` | 实现 PlatformRegistry |
| 4 | `im/session.rs` | 实现 SessionSource + 会话查找逻辑 |
| 5 | `im/gateway.rs` | 实现 IMGateway（注册 + 路由 + Agent 对接） |
| 6 | `im/mod.rs` | 模块导出 + 初始化函数 |
| 7 | `dingtalk/adapter.rs` | 实现 DingTalkAdapter implements IMAdapter |
| 8 | `tools/builtin.rs` | 注册 send_im_message 工具 |
| 9 | `lib.rs` | initialize() 中初始化 IMGateway 并注册各适配器 |
| 10 | 编译验证 | `cargo check` 确保无错误 |

---

## 十一、对比参考：两大项目的优缺取舍

| 借鉴点 | 来源 | 在本方案中的体现 |
|--------|------|----------------|
| 适配器契约接口 | Hermes `BasePlatformAdapter` | `IMAdapter trait` |
| 平台能力声明 | OpenClaw `ChannelCapabilities` | `PlatformCapabilities` 结构体 |
| 注册中心 + 两级查找 | Hermes `PlatformRegistry` | `HashMap<PlatformType, Arc<dyn IMAdapter>>` |
| 标准化入站消息 | Hermes `MessageEvent` | `IncomingMessage` 结构体 |
| 跨平台会话标识 | Hermes `SessionSource` | `SessionSource` 结构体 |
| 平台类型枚举 + 自定义 | Hermes `Platform._missing_()` | `PlatformType::Custom(String)` |
| 延迟加载 | OpenClaw 插件系统 | 暂不实现，启动时直接注册 |
| 插件式架构 | OpenClaw 扩展目录 | 未来可按需添加（每个平台一个目录） |
| 投递路由语法 | Hermes `DeliveryRouter` | 暂不实现，后续可按需添加 |
| 生命周期钩子 | Hermes `hooks.py` | 暂不实现，保持简单 |

---

## 十二、注意事项

1. **DingTalk 私聊/群聊区分**：钉钉的私聊和群聊使用不同的 REST API 端点，`MessageTarget` 需要额外字段 `conversation_type` 或通过 `conversation_id` 前缀判断
2. **Token 缓存一致性**：多个适配器不应各自维护 Token Manager，应由 IMGateway 统一管理或由各适配器独立管理（取决于平台是否共享 Token）
3. **agent 回复路径**：Agent 在 ReAct 循环中直接回复时（无工具调用），回复内容需由发起请求的上层（IMGateway/HTTP handler）负责发送回 IM 平台
4. **线程安全**：`IMAdapter` 要求 `Send + Sync`，所有实现需确保内部状态使用 `Arc<RwLock<>>` 或 `Atomic*` 保护
