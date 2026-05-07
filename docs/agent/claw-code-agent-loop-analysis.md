# claw-code（Rust 版 Claude Code）Agent 机制深度解读

> ReAct + Agent Loop + CoT 完整实现分析

---

## 一、项目概览

**claw-code** 是由 AI Agent（crabs/claws）自主构建的 Rust 版 Claude Code 替代品，采用 Workspace 架构，Python `src/` 目录为归档参考实现，**Rust `rust/crates/` 才是正式代码库**。

### 核心特色

- 🦀 **纯 Rust 实现**：tokio 异步运行时 + reqwest HTTP
- 🏗️ **Workspace 架构**：`api` / `runtime` / `tools` / `commands` / `plugins` / `telemetry` 六个 crate
- 🤖 **完整 ReAct 循环**：`ConversationRuntime::run_turn()` 一行代码启动完整推理-工具循环
- 🔗 **原生 Anthropic API**：支持 Extended Thinking + Prompt Caching + OAuth
- 💾 **嵌入式存储**：JSONL 文件持久化（零外部依赖）
- 🔒 **安全沙箱**：PermissionPolicy + 三层 Hook + HookAbortSignal
- 📊 **可观测性**：SessionTracer + AnalyticsEvent + Usage Tracker

---

## 二、整体架构

```
主要 CLI ──→ 命令路由
               │
    ┌──────────┼──────────┐
    │          │          │
    ▼          ▼          ▼
  REPL    一次性命令   服务模式
    │
    ▼
  clawd run ──→ ConversationRuntime::run_turn()
                     │
    ┌────────────────┼────────────────────┐
    │                │                    │
    ▼                ▼                    ▼
ApiClient        ToolExecutor       PermissionPolicy
(Anthropic)      (Tool Registry)    (三层Hook)
```

### Crate 架构

| Crate | 职责 | 关键模块 |
|-------|------|---------|
| `api` | API 客户端 (Anthropic/OpenAI兼容) | `providers/anthropic.rs`, `types.rs`, `sse.rs`, `prompt_cache.rs` |
| `runtime` | **核心运行时（Agent Loop）** | `conversation.rs`, `session.rs`, `compact.rs`, `prompt.rs`, `permissions.rs`, `hooks.rs` |
| `tools` | 工具注册/分发/执行 | `lib.rs` (Tool trait) |
| `commands` | CLI 命令定义与路由 | `lib.rs` |
| `plugins` | 插件生命周期与 Hook | `hooks.rs`, `test_isolation.rs` |
| `telemetry` | 遥测与可观测性 | `lib.rs` (SessionTracer, Analytics) |

---

## 三、Agent Loop（ReAct 循环）详解

### 3.1 核心入口：`ConversationRuntime::run_turn()`

这是整个项目的灵魂——**一行调用即可启动完整的 ReAct 循环**：

```rust
// src/crates/runtime/src/conversation.rs - 第 314 行
pub fn run_turn(
    &mut self,
    user_input: impl Into<String>,
    mut prompter: Option<&mut dyn PermissionPrompter>,
) -> Result<TurnSummary, RuntimeError> {
```

**调用链路**：
```
CLI 输入 → clawd run → ConversationRuntime::run_turn("用户消息", &mut prompter)
```

### 3.2 ReAct 循环完整流程图

```
run_turn(user_input, prompter)
│
├─ 0. Session-health canary (ROADMAP #38)
│     if self.session.compaction.is_some() {
│         self.run_session_health_probe() → 验证工具执行器可用
│     }
│
├─ 1. 记录 turn_started
│     self.session.push_user_text(user_input)  ← Observation: 接收输入
│
└─ 2. ReAct 主循环: loop {}
         │
         ├─ iterations++ (防死循环)
         │
         ├─ ❓ iterations > max_iterations? → 返回错误
         │
         ├─ ▸ Thought + Action: 调用 LLM
         │     let request = ApiRequest { system_prompt, messages }
         │     let events = self.api_client.stream(request)  ← 流式
         │     let (assistant_message, usage, cache_events) = build_assistant_message(events)
         │
         ├─ ▸ 提取 Tool Uses
         │     let pending_tool_uses = assistant_message.blocks.iter()
         │         .filter_map(|block| match block {
         │             ContentBlock::ToolUse { id, name, input } => Some(...)
         │             _ => None
         │         })
         │
         ├─ ▸ 推送 assistant message 到 session
         │     self.session.push_message(assistant_message)
         │
         ├─ ❓ pending_tool_uses.is_empty()? → break (任务完成!)
         │
         └─ ▸ Action + Observation: 逐个执行工具
              for (tool_use_id, tool_name, input) in pending_tool_uses {
                  │
                  ├─ 1) PreToolUse Hook (可能 deny/cancel)
                  ├─ 2) Permission Check (PermissionPolicy)
                  ├─ 3) PermissionOutcome::Allow → 执行工具
                  │       self.tool_executor.execute(&tool_name, &input)
                  ├─ 4) PostToolUse Hook 或 Failure Hook
                  └─ 5) 推入 tool_result 到 session
              }
              continue → 回到 loop{} 顶部（下一轮 Thought）
```

### 3.3 关键状态结构

```rust
// TurnSummary - 一轮对话的完整结果
pub struct TurnSummary {
    pub assistant_messages: Vec<ConversationMessage>,  // 各轮助手消息
    pub tool_results: Vec<ConversationMessage>,         // 工具执行结果
    pub prompt_cache_events: Vec<PromptCacheEvent>,     // 缓存事件
    pub iterations: usize,                              // 实际迭代次数
    pub usage: TokenUsage,                               // Token 使用统计
    pub auto_compaction: Option<AutoCompactionEvent>,    // 自动压缩事件
}
```

### 3.4 退出条件矩阵

| 退出条件 | 实现 | reason |
|---------|------|--------|
| 无工具调用 | `pending_tool_uses.is_empty() → break` | 自然完成 |
| 超迭代次数 | `iterations > self.max_iterations` | `RuntimeError` |
| API 失败 | `api_client.stream()` 返回 Err | `RuntimeError` |
| 工具执行器损坏 | `run_session_health_probe() → Err` | `RuntimeError` |
| Hook 取消/拒绝 | PermissionOutcome::Deny → tool_result(is_error:true) | 继续但不执行 |

### 3.5 迭代计数器

```rust
let mut iterations = 0;

loop {
    iterations += 1;
    if iterations > self.max_iterations {
        // 防止死循环: max_iterations 默认 usize::MAX
        return Err(RuntimeError::new(
            "conversation loop exceeded the maximum number of iterations"
        ));
    }
    // ...
}
```

---

## 四、CoT（Chain of Thought / Extended Thinking）实现

### 4.1 Anthropic Extended Thinking 集成

claw-code 通过 `api` crate 的 `AnthropicRequestProfile` 启用 Extended Thinking：

```rust
// src/crates/api/src/providers/anthropic.rs
pub fn render_json_body(&self, request: &MessageRequest) -> Result<Value, ApiError> {
    let mut body = Map::new();
    // ...
    if let Some(thinking) = &request.thinking {
        // 注入 thinking 配置到请求体
        body.insert("thinking".to_string(), serde_json::to_value(thinking)?);
    }
    // ...
}
```

### 4.2 Thinking 配置类型

```rust
// src/crates/api/src/types.rs
pub struct ThinkingConfig {
    pub type: ThinkingType,      // "enabled" | "disabled"
    pub budget_tokens: u32,       // thinking token 预算（如 16000）
}

pub enum ThinkingType {
    Enabled,
    Disabled,
}

// Request 中的 thinking 字段
pub struct MessageRequest {
    // ...
    pub thinking: Option<ThinkingConfig>,  // ← CoT 配置
    // ...
}
```

### 4.3 API 响应中的 Thinking 块

Anthropic Extended Thinking API 返回的响应包含特殊 content block 类型：

```rust
// src/crates/api/src/types.rs
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: Json },
    Thinking { thinking: String, signature: String },      // ← CoT 推理
    RedactedThinking { data: String },                       // ← 安全过滤后的推理
    // ...
}
```

流式响应中的 thinking delta：
```rust
pub enum ContentBlockDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },     // ← CoT 流式增量
    SignatureDelta { signature: String },    // ← 签名验证
}
```

### 4.4 Thinking 在 Agent Loop 中的应用

```
LLM 流式输出:
  content_block_start: { type: "thinking" }
  content_block_delta: { thinking: "I need to analyze..." }     ← CoT 推理
  content_block_delta: { thinking: "The user wants to..." }
  content_block_stop
  content_block_start: { type: "text" }
  content_block_delta: { text: "Based on my analysis..." }      ← 用户可见
  ...
  content_block_start: { type: "tool_use", id: "tool-1", name: "bash" }
  content_block_delta: { partial_json: "{\"command\":\"ls\"}" }
  ...
  message_stop
```

**注意**：在当前 `ConversationRuntime` 实现中，AssistantEvent 枚举只处理了 `TextDelta`、`ToolUse`、`Usage`、`PromptCache`、`MessageStop` 五种事件。Thinking 块在 API 层已被正确处理，但在 Runtime 层目前被解析但不做特殊处理（作为普通 content block 存入 session）。

---

## 五、流式 API 集成

### 5.1 三层抽象

```
ConversationRuntime::run_turn()
    │
    ├─ ApiClient trait (delegates to Provider)
    │     └─ AnthropicClient::stream_message()
    │           └─ MessageStream::next_event()
    │                 ├─ SSE Parser (分帧)
    │                 ├─ Content Block 组装
    │                 └─ Usage tracking
    │
    └─ build_assistant_message(events)
          └─ 将 StreamEvent[] → ConversationMessage
```

### 5.2 SSE 解析器

```rust
// src/crates/api/src/sse.rs
pub struct SseParser {
    buffer: String,
    // ...
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<StreamEvent>, ApiError> {
        // 1. 追加 chunk 到 buffer
        // 2. 按双换行分割 frame
        // 3. 解析 event: / data: 行
        // 4. 组装事件
    }
}
```

### 5.3 事件组装

```rust
// conversation.rs - build_assistant_message()
fn build_assistant_message(events: Vec<AssistantEvent>) -> Result<(
    ConversationMessage,    // 组装好的消息
    Option<TokenUsage>,     // Token 用量
    Vec<PromptCacheEvent>,  // 缓存事件
), RuntimeError> {

    let mut text = String::new();
    let mut blocks = Vec::new();

    for event in events {
        match event {
            TextDelta(delta) → text.push_str(&delta),
            ToolUse { id, name, input } → {
                flush_text_block(&mut text, &mut blocks);  // 先保存文本块
                blocks.push(ContentBlock::ToolUse { id, name, input });
            }
            Usage(value) → usage = Some(value),
            PromptCache(event) → cache_events.push(event),
            MessageStop → finished = true,
        }
    }

    // 最终验证
    if !finished → Err("no message stop event")
    if blocks.is_empty() → Err("no content")
}
```

---

## 六、上下文压缩（Compaction）

### 6.1 三层压缩策略

| 策略 | 触发时机 | 实现 |
|------|---------|------|
| **Auto Compaction** | 每轮 `run_turn()` 结束后 | `maybe_auto_compact()`: `input_tokens >= threshold` → `compact_session()` |
| **Manual Compaction** | 外部调用 `runtime.compact(config)` | 保留最近 N 条消息，其余生成摘要 |
| **Health Probe** | 压缩后首轮对话前 | `run_session_health_probe()`: 验证工具执行器可用 |

### 6.2 Auto Compaction 阈值

```rust
// 环境变量: CLAUDE_CODE_AUTO_COMPACT_INPUT_TOKENS
// 默认值: 100,000 tokens
const DEFAULT_AUTO_COMPACTION_INPUT_TOKENS_THRESHOLD: u32 = 100_000;
```

### 6.3 压缩算法

```rust
// compact.rs
pub fn compact_session(session: &Session, config: CompactionConfig) -> CompactionResult {
    // 1. 判断是否需要压缩
    //    should_compact(session, config)
    //      → compactable.len() > preserve_recent_messages
    //        && token_count >= max_estimated_tokens

    // 2. 分割为 [旧消息] + [保留消息]
    //    旧消息 = messages[0..compact_index]
    //    保留消息 = messages[compact_index..]

    // 3. 生成摘要（调用 LLM 或简单拼接）
    //    summary = "<summary>..." + "</summary>"

    // 4. 构造压缩后的 Session
    //    compacted_session = Session {
    //        messages: [System(摘要), ...保留消息],
    //        compaction: Some(SessionCompaction { count, summary, ... }),
    //    }
}
```

### 6.4 压缩后续指令

```rust
const COMPACT_CONTINUATION_PREAMBLE: &str =
    "This session is being continued from a previous conversation that ran out of context...";

const COMPACT_DIRECT_RESUME_INSTRUCTION: &str =
    "Continue the conversation from where it left off without asking the user any further questions. \
     Resume directly — do not acknowledge the summary...";
```

---

## 七、提示词工程

### 7.1 SystemPromptBuilder

```rust
// src/crates/runtime/src/prompt.rs
pub struct SystemPromptBuilder {
    output_style_name: Option<String>,
    output_style_prompt: Option<String>,
    os_name: Option<String>,
    os_version: Option<String>,
    append_sections: Vec<String>,
    project_context: Option<ProjectContext>,  // CWD, git status, diff
    config: Option<RuntimeConfig>,
}

impl SystemPromptBuilder {
    pub fn build(&self) -> Vec<String> {
        // 静态内容部分
        sections.push(get_simple_intro_section());    // "你是 Claude Code..."
        sections.push(get_simple_system_section());   // 系统规则
        sections.push(get_tool_use_section());         // 工具使用指南

        // 动态边界
        sections.push(SYSTEM_PROMPT_DYNAMIC_BOUNDARY);

        // 动态内容部分（包含环境/配置）
        sections.push(render_environment_section());
        sections.push(render_project_context());
        // ...
    }
}
```

### 7.2 静态/动态分离

```
┌─────────────────────────────┐
│  静态 Prompt（可全局缓存）    │ ← PromptCache 缓存
│  ├─ Intro Section            │
│  ├─ System Section           │
│  ├─ Tool Use Section         │
│  └─ Tone & Style Section     │
├─────────────────────────────┤
│  SYSTEM_PROMPT_DYNAMIC_BOUND │ ← 缓存边界
├─────────────────────────────┤
│  动态 Prompt（会话相关）      │ ← 不缓存
│  ├─ Environment Section      │
│  ├─ Project Context          │
│  ├─ Language Preference      │
│  └─ Output Style             │
└─────────────────────────────┘
```

### 7.3 Prompt Cache

```rust
// src/crates/api/src/prompt_cache.rs
pub struct PromptCache { /* ... */ }

impl PromptCache {
    /// 查找缓存的完整响应（相同 prompt → 复用结果）
    pub fn lookup_completion(&self, request: &MessageRequest) -> Option<MessageResponse>;

    /// 记录 API 响应到缓存
    pub fn record_response(&self, request: &MessageRequest, response: &MessageResponse) -> PromptCacheRecord;

    /// 仅记录 Usage 不缓存响应
    pub fn record_usage(&self, request: &MessageRequest, usage: &Usage) -> PromptCacheRecord;

    /// 缓存统计
    pub fn stats(&self) -> PromptCacheStats;
}
```

---

## 八、安全机制

### 8.1 三层 Hook 模型

```
       ┌─────────────────┐
       │   Tool Request   │
       └────────┬────────┘
                ↓
    ┌───────────────────────┐
    │  PreToolUse Hook      │  ← 第一道防线
    │  - 可修改输入 (updated_input) │
    │  - 可拒绝 (deny)      │
    │  - 可取消 (cancel)    │
    │  - 可失败 (fail)      │
    └────────┬──────────────┘
             ↓
    ┌───────────────────────┐
    │  PermissionPolicy     │  ← 第二道防线
    │  - DangerFullAccess   │
    │  - WorkspaceWrite     │
    │  - ReadOnly           │
    │  - Custom Rules       │
    └────────┬──────────────┘
             ↓
    ┌───────────────────────┐
    │  Tool.execute()       │
    └────────┬──────────────┘
             ↓
    ┌───────────────────────┐
    │  PostToolUse Hook     │  ← 第三道防线（成功）
    │  PostToolUseFailure   │  ← 第三道防线（失败）
    └───────────────────────┘
```

### 8.2 HookAbortSignal

```rust
pub struct HookAbortSignal {
    signal: Arc<tokio::sync::watch::Sender<bool>>,
}

// 紧急中断所有活跃 Hook
// 用法: signal.abort() → 所有 hook 进程收到终止信号
```

### 8.3 Hook Feedback 合并

```rust
// 工具输出 + Hook 反馈 → 最终消息
fn merge_hook_feedback(messages: &[String], output: String, is_error: bool) -> String {
    if messages.is_empty() { return output; }
    // "tool output\n\nHook feedback:\n{message1}\n{message2}"
    format!("{tool_output}\n\nHook feedback:\n{}", messages.join("\n"))
}
```

---

## 九、消息数据结构

### 9.1 四种角色 → ReAct 四阶段

```rust
pub enum MessageRole {
    System,     // 系统提示词
    User,       // 用户输入 (ReAct 输入)
    Assistant,  // 模型输出 (Thought + Action)
    Tool,       // 工具结果 (Observation)
}

pub enum ContentBlock {
    Text { text: String },                                        // 纯文本
    ToolUse { id: String, name: String, input: String },          // 工具调用
    ToolResult { tool_use_id: String, tool_name: String,
                output: String, is_error: bool },                  // 工具结果
}
```

### 9.2 消息示例

```
一轮完整 ReAct 的消息序列:
[
  User:        [{ Text: "what is 2 + 2?" }],           ← ReAct: 输入
  Assistant:   [{ Text: "thinking" },                   ← ReAct: Thought
                { ToolUse: { id:"tool-1", name:"add",
                             input:"2,2" } }],          ← ReAct: Action
  Tool:        [{ ToolResult: { tool_use_id:"tool-1",
                                output:"4",
                                is_error: false } }],   ← ReAct: Observation
  Assistant:   [{ Text: "The answer is 4." }],          ← ReAct: Final Answer
]
```

---

## 十、Session 持久化

### 10.1 JSONL 流式追加

```rust
impl Session {
    pub fn with_persistence_path(path) → Session         // 绑定文件
    pub fn save_to_path(path) → full snapshot            // 全量保存
    pub fn load_from_path(path) → Session                // 加载恢复

    fn append_persisted_message(msg) → append one line   // 增量追加
}

// 存储格式: 每行一条 JSONL 记录
// {"type":"session_meta","version":1,"session_id":"...",...}
// {"type":"message","message":{"role":"user","blocks":[...]}}
// {"type":"message","message":{"role":"assistant","blocks":[...]}}
// {"type":"compaction","count":1,"summary":"...",...}
```

### 10.2 日志轮转

```rust
const ROTATE_AFTER_BYTES: u64 = 256 * 1024;   // 256KB 轮转
const MAX_ROTATED_FILES: usize = 3;            // 最多保留 3 份历史
```

### 10.3 Session Fork

```rust
// 从当前 Session 分叉（保留所有历史）
let forked = runtime.fork_session(Some("investigation"));

// forked:
//   - 新的 session_id
//   - fork: Some({ parent_session_id, branch_name: "investigation" })
//   - messages: 继承所有历史
//   - workspace_root: 继承
```

---

## 十一、整体数据流总结

```
┌────────────────────────────────────────────────────────────┐
│  clawd run → ConversationRuntime::run_turn()               │
│                                                             │
│  ┌─ Session-health Probe ──────────────────────────────┐   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─ 1. push_user_text("what is 2 + 2?") ───────────────┐   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─ 2. ReAct Loop ─────────────────────────────────────┐   │
│  │  loop {                                                │   │
│  │    ├─ ApiRequest { system_prompt, messages }           │   │
│  │    ├─ AnthropicClient::stream_message()                │   │
│  │    │   ├─ preflight (token count + byte estimate)      │   │
│  │    │   ├─ send_with_retry (指数退避 + jitter)           │   │
│  │    │   └─ MessageStream::next_event()                  │   │
│  │    │       ├─ SSE Parse                                │   │
│  │    │       ├─ Content Block Assembly                   │   │
│  │    │       ├─ Usage Tracking                           │   │
│  │    │       └─ PromptCache Record                       │   │
│  │    │                                                     │
│  │    ├─ build_assistant_message() → ConversationMessage   │   │
│  │    ├─ Extract pending_tool_uses                        │   │
│  │    ├─ ❓ tool_uses.is_empty()? → break                  │   │
│  │    │                                                     │
│  │    └─ for each tool_use:                                │   │
│  │        ├─ PreToolUse Hook (可修改输入/拒绝)             │   │
│  │        ├─ PermissionPolicy.authorize()                  │   │
│  │        ├─ tool_executor.execute(name, input)            │   │
│  │        ├─ PostToolUse / Failure Hook                    │   │
│  │        └─ merge_hook_feedback → tool_result message     │   │
│  │  }                                                       │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─ 3. maybe_auto_compact() ───────────────────────────┐   │
│  │  if input_tokens >= threshold:                         │   │
│  │    compact_session() → 替换 self.session               │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  return TurnSummary { iterations, usage, ... }              │
└────────────────────────────────────────────────────────────┘
```

---

## 十二、与其他实现对比

| 特性 | cc-haha (TS) | claude-code-rust (Rust) | claw-code (Rust) |
|------|-------------|------------------------|------------------|
| **ReAct 循环** | ✅ while(true) | ❌ 单次调用 | ✅ loop{} + tool迭代 |
| **CoT/Thinking** | ✅ adaptive/enabled/disabled | ❌ 未实现 | ✅ Anthropic Extended Thinking (API层) |
| **流式执行** | ✅ StreamingToolExecutor | ❌ | ✅ tokio + async stream |
| **上下文压缩** | ✅ 6层 | ✅ ContextWindow | ✅ Auto + Manual + Health |
| **权限控制** | ✅ | ❌ | ✅ 三层Hook + PermissionPolicy |
| **Session 持久化** | ✅ JSONL | ❌ | ✅ JSONL + 增量追加 + 轮转 |
| **OAuth** | ✅ | ❌ | ✅ BearerToken + Refresh |
| **Prompt Cache** | ✅ | ❌ | ✅ PromptCache (lookup + record) |
| **可观测性** | ✅ SessionTracer | ❌ | ✅ SessionTracer + Analytics |

---

## 十三、关键启示（对 Hclaw 项目的参考价值）

### 13.1 ReAct Loop 架构模式

```rust
// 推荐 Hclaw 采用的结构
pub struct ConversationRuntime<C: ApiClient, T: ToolExecutor> {
    session: Session,
    api_client: C,
    tool_executor: T,
    permissions: PermissionPolicy,
    system_prompt: Vec<String>,
    max_iterations: usize,
    usage_tracker: UsageTracker,
    // ...
}

impl ConversationRuntime {
    pub fn run_turn(&mut self, user_input: &str) -> Result<TurnSummary> {
        self.session.push_user_text(user_input);
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > self.max_iterations { return Err(/*...*/); }

            // 1. Thought + Action
            let request = ApiRequest {
                system_prompt: self.system_prompt.clone(),
                messages: self.session.messages.clone(),
            };
            let events = self.api_client.stream(request)?;
            let assistant_msg = build_assistant_message(events)?;
            self.session.push_message(assistant_msg.clone());

            // 2. Extract tool uses
            let tool_uses: Vec<_> = assistant_msg.blocks.iter()
                .filter_map(|b| match b { ToolUse {..} => Some(/*...*/), _ => None })
                .collect();

            // 3. No tool uses → done
            if tool_uses.is_empty() { break; }

            // 4. Observation: execute tools
            for tool in tool_uses {
                // permission check → execute → record result
                let output = self.tool_executor.execute(&tool.name, &tool.input)?;
                self.session.push_message(
                    ConversationMessage::tool_result(tool.id, tool.name, output)
                );
            }
            // → continue loop
        }

        Ok(TurnSummary { iterations, /*...*/ })
    }
}
```

### 13.2 值得借鉴的设计

1. **`run_turn()` 入口极简**：一行调用完成全部 ReAct，外部无需理解循环细节
2. **Session = 完整状态**：消息、压缩、分叉、工作区根都在 Session 中
3. **JSONL 增量追加**：每次 push_message 自动追加一行，崩溃不丢数据
4. **三层 Hook 按 Cost 排列**：Pre→Permission→Tool→Post→Failure
5. **Static/Dynamic Prompt Boundary**：明确缓存边界，无需复杂缓变计算

