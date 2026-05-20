# codex-rs（OpenAI Codex CLI）Agent 机制深度解读

> ReAct + Agent Loop + CoT（Chain of Thought / Reasoning）完整实现分析

---

## 一、项目概览

**codex-rs** 是 **OpenAI Codex CLI** 的 Rust 实现，采用 Cargo Workspace 架构，包含 **90+ crate**。这是目前分析的四个项目中**规模最大、成熟度最高、架构最完整**的 Agent 系统。

### 核心特色

- 🏭 **90+ Crate 的 Monorepo**：`core` / `app-server` / `codex-api` / `tools` / `exec` / `protocol` / `tui` / `skills` / `plugins` 等
- 🔐 **企业级安全**：沙箱化执行（Windows Sandbox + Linux Sandbox）、三层 Hook 模型
- 🧠 **原生 OpenAI Reasoning**：支持 `reasoning_effort` + `reasoning_summary` + 流式内容
- 🔌 **MCP 生态集成**：完整的 MCP 客户端/服务端，支持 connectored App 体系
- 📡 **多协议通信**：WebSocket (Realtime API) + HTTPS 双模自动切换，带指数退避重试
- 📊 **全链路追踪**：OpenTelemetry + SessionTelemetry + Analytics 事件系统

---

## 二、整体架构

```
codex-rs/  (Cargo Workspace)
├── core/          ← 🧠 核心 Agent 引擎 (Agent Loop + ReAct + Session)
├── app-server/    ← 🌐 HTTP / WebSocket 服务 (turn_start, turn_steer, compact)
├── codex-api/     ← 📡 API 客户端 (OpenAI Responses API + Realtime WebSocket)
├── tools/         ← 🔧 工具抽象与注册 (ToolSpec, ToolRegistry)
├── protocol/      ← 📨 通信协议与事件类型 (EventMsg, ResponseItem, TurnItem)
├── exec/          ← ⚙️  执行引擎 (命令执行、Shell 管理)
├── exec-server/   ← 🖥️ 远程执行服务器
├── skills/        ← 🎯 技能系统 (发现、加载、依赖解析)
├── plugins/       ← 🔌 插件市场 (安装、启停、Hook)
├── hooks/         ← 🪝 Hook 调度引擎 (Pre/Post/Stop/PendingInput)
└── tui/           ← 🖼️ TUI 用户界面 (crossterm + ratatui)
```

### Crate 职责矩阵

| Crate | 职责 | 关键文件 |
|-------|------|---------|
| `core` | **Agent 引擎**：控制层、Mailbox、Session 管理、Turn 执行、工具路由 | `session/turn.rs`, `agent/`, `tools/router.rs` |
| `app-server` | HTTP 服务：turn_start/turn_steer/compact，权限审批、UI 事件流 | `request_processors/turn_processor.rs` |
| `codex-api` | API 客户端：OpenAI Responses API、Realtime WebSocket、SSE 流、OAuth | `endpoint/responses.rs` |
| `tools` | 工具规范：ToolSpec、ToolRegistry、DiscoverableTool、CodeMode 工具 | `lib.rs` |
| `protocol` | 通信层：EventMsg、ResponseItem、TurnItem、所有 Delta 事件 | `lib.rs` |
| `exec` | 执行引擎：Shell 管理、审批策略、路径安全 | `lib.rs` |

---

## 三、Agent Loop（ReAct 循环）详解

### 3.1 核心入口：`run_turn()`

```rust
// core/src/session/turn.rs - 第 138 行
/// 接收用户输入，运行如下循环：
/// 每次 sampling request，模型回复：
///   - 工具调用 (function calls) → 执行并反馈结果 → 继续循环
///   - 助手消息 (assistant message) → 记录并结束 turn
pub(crate) async fn run_turn(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
    prewarmed_client_session: Option<ModelClientSession>,
    cancellation_token: CancellationToken,
) -> Option<String>
```

### 3.2 完整 ReAct 循环流程图

```
run_turn(sess, turn_context, input, prewarmed_client, cancel_token)
│
├─ 0️⃣ Empty input check
│     if input.is_empty() && !sess.has_pending_input() → return None
│
├─ 1️⃣ Pre-sampling compaction
│     run_pre_sampling_compact() → 检查 token 使用量 → 必要时压缩
│
├─ 2️⃣ Turn preparation
│     ├─ record_context_updates_and_set_reference()
│     ├─ build_skill_injections()        → 技能项注入
│     ├─ build_plugin_injections()        → 插件项注入
│     ├─ run_pending_session_start_hooks()
│     ├─ run_user_prompt_submit_hooks()   → 用户提交 Hook
│     └─ record_user_prompt_and_emit_turn_item()
│
└─ 3️⃣ 🧠 ReAct 主循环: loop {}
         │
         ├─ run_pending_session_start_hooks()
         │
         ├─ ▸ 获取 Pending Input (UI异步消息)
         │     inspect_pending_input() → Accepted/Blocked
         │     record_pending_input()   → 追加到 session
         │
         ├─ ▸ 构建 prompt
         │     build_prompt(input, tools, base_instructions, personality, output_schema)
         │
         ├─ ▸ Thought + Action: run_sampling_request()
         │    │
         │    ├─ built_tools() → ToolRouter (含 model_visible_specs)
         │    ├─ ToolCallRuntime::new()  → 工具运行时
         │    └─ loop (重试循环):
         │         └─ try_run_sampling_request()
         │              │
         │              ├─ client_session.stream(prompt, reasoning_effort, reasoning_summary)
         │              │   → WebSocket 优先 → HTTPS fallback
         │              │
         │              └─ loop (事件处理):
         │                   ├─ ResponseEvent::Created
         │                   ├─ ResponseEvent::OutputItemAdded
         │                   │   ├─ AgentMessage → 流式文本处理
         │                   │   ├─ Reasoning → reasoning delta 处理  ← CoT
         │                   │   ├─ FunctionCall → tool diff consumer
         │                   │   └─ LocalShellCall → shell diff consumer
         │                   │
         │                   ├─ ResponseEvent::ContentDelta / TextDelta
         │                   ├─ ResponseEvent::FunctionCallArgumentsDelta
         │                   ├─ ResponseEvent::ReasoningContentDelta     ← CoT 流式
         │                   ├─ ResponseEvent::ReasoningSummaryDelta     ← CoT 摘要
         │                   ├─ ResponseEvent::ReasoningSummaryPartAdded ← CoT 分段
         │                   │
         │                   └─ ResponseEvent::OutputItemDone
         │                        ├─ handle_output_item_done()
         │                        ├─ if tool_call → push to in_flight (FuturesOrdered)
         │                        ├─ needs_follow_up |= true
         │                        └─ Mailbox 检查 → 可提前退出
         │
         ├─ ▸ 返回 SamplingRequestResult { needs_follow_up, last_agent_message }
         │
         ├─ ▸ Token 检查
         │     total_usage_tokens >= auto_compact_limit && needs_follow_up?
         │       → run_auto_compact() → continue
         │
         ├─ ▸ ❓ !needs_follow_up && !has_pending_input?
         │     ├─ run stop_hook → block (add prompt→continue) | stop (break) | pass
         │     ├─ run after_agent_hook → success/fail_continue/fail_abort
         │     └─ break → turn完成
         │
         └─ ▸ has_pending_input? → can_drain_pending_input = true → continue
```

### 3.3 关键状态：`SamplingRequestResult`

```rust
struct SamplingRequestResult {
    needs_follow_up: bool,              // ← 是否有工具调用需要继续
    last_agent_message: Option<String>,  // ← 最终助手消息文本
}
```

### 3.4 `try_run_sampling_request()` —— 单次 LLM 调用

```rust
// core/src/session/turn.rs - 第 1841 行
async fn try_run_sampling_request(
    tool_runtime: ToolCallRuntime,
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    client_session: &mut ModelClientSession,
    turn_metadata_header: Option<&str>,
    turn_diff_tracker: SharedTurnDiffTracker,
    prompt: &Prompt,
    cancellation_token: CancellationToken,
) -> CodexResult<SamplingRequestResult> {

    // 1. 启动流式请求（WebSocket 优先）
    let mut stream = client_session.stream(
        prompt,
        &turn_context.model_info,
        &turn_context.session_telemetry,
        turn_context.reasoning_effort,      // ← CoT 推理强度
        turn_context.reasoning_summary,      // ← CoT 摘要模式
        turn_context.config.service_tier,
        turn_metadata_header,
        &inference_trace,
    ).instrument(trace_span!("stream_request")).await??;

    // 2. 并发工具调用容器
    let mut in_flight = FuturesOrdered::new();

    // 3. 事件处理主循环
    loop {
        let event = stream.next().await?;   // ← 流式读取

        match event {
            ResponseEvent::OutputItemDone(item) => {
                // 处理完成的响应项
                let output_result = handle_output_item_done(&mut ctx, item, previous).await?;
                if let Some(tool_future) = output_result.tool_future {
                    in_flight.push_back(tool_future);  // ← 异步工具执行
                }
                needs_follow_up |= output_result.needs_follow_up;
            }
            ResponseEvent::ReasoningContentDelta { .. }  => // ← CoT: 原始推理
            ResponseEvent::ReasoningSummaryDelta { .. }   => // ← CoT: 推理摘要
            ResponseEvent::ReasoningSummaryPartAdded { .. } => // ← CoT: 推理分段
            // ...
        }
    }
}
```

### 3.5 退出条件矩阵

| 退出条件 | 判断逻辑 | 处理方式 |
|---------|---------|---------|
| 无工具调用 | `!needs_follow_up && !has_pending_input` | break → 完成 |
| Token 超限 + 需继续 | `token >= limit && needs_follow_up` | auto_compact → continue |
| Stop Hook block | `stop_outcome.should_block` | 追加 hook prompt → continue |
| Stop Hook stop | `stop_outcome.should_stop` | break → 终止 |
| AfterAgent Hook abort | `HookResult::FailedAbort` | return None → 中断 |
| 用户取消 | `TurnAborted` | break |
| 无效图片 | `InvalidImageRequest` | 清理图片 → continue |
| 流错误 (retryable) | `err.is_retryable()` | 指数退避重试 → continue |
| WebSocket → HTTPS fallback | `retries >= max && switch` | 切换传输层 → continue |
| Session start hook 失败 | `run_pending_session_start_hooks` = true | break |

### 3.6 重试机制（指数退避 + 传输层切换）

```rust
// core/src/session/turn.rs - 第 1084-1138 行
if !err.is_retryable() { return Err(err); }

let max_retries = turn_context.provider.info().stream_max_retries();

// 1. 先尝试 WebSocket → HTTPS fallback
if retries >= max_retries && client_session.try_switch_fallback_transport() {
    retries = 0;
    continue;  // 用 HTTPS 重试
}

// 2. 指数退避重试
if retries < max_retries {
    retries += 1;
    let delay = err.requested_delay.unwrap_or_else(|| backoff(retries));
    tokio::time::sleep(delay).await;
    sess.notify_stream_error("Reconnecting...");
    continue;
}
```

---

## 四、CoT（Chain of Thought / Reasoning）实现

### 4.1 OpenAI Reasoning API 集成

codex-rs 使用 OpenAI 的 **reasoning_effort** 参数（用于 o1/o3 等推理模型）：

```rust
// core/src/session/turn.rs - 第 1864-1874 行
let mut stream = client_session.stream(
    prompt,
    &turn_context.model_info,
    &turn_context.session_telemetry,
    turn_context.reasoning_effort,      // ← "low" | "medium" | "high"
    turn_context.reasoning_summary,      // ← summary format (auto/text)
    turn_context.config.service_tier,
    turn_metadata_header,
    &inference_trace,
);
```

### 4.2 三种推理 Delta 事件

| 事件类型 | 含义 | 协议类型 |
|---------|------|---------|
| `ReasoningContentDelta` | 原始推理 token（流式增量） | `ReasoningRawContentDeltaEvent` |
| `ReasoningSummaryDelta` | 推理摘要增量 | `ReasoningContentDeltaEvent` |
| `ReasoningSummaryPartAdded` | 推理分段标记 | `AgentReasoningSectionBreakEvent` |

```rust
// 推理摘要流式处理
ResponseEvent::ReasoningSummaryDelta { delta, summary_index } => {
    let event = ReasoningContentDeltaEvent {
        thread_id, turn_id, item_id,
        delta, summary_index,
    };
    sess.send_event(&turn_context, EventMsg::ReasoningContentDelta(event)).await;
}

// 原始推理流式处理
ResponseEvent::ReasoningContentDelta { delta, content_index } => {
    let event = ReasoningRawContentDeltaEvent {
        thread_id, turn_id, item_id,
        delta, content_index,
    };
    sess.send_event(&turn_context, EventMsg::ReasoningRawContentDelta(event)).await;
}
```

### 4.3 Reasoning Item 类型

```rust
// 流式响应中的推理项
pub enum ResponseItem {
    Reasoning { ... },     // ← 专用推理项类型
    // 在 handle_output_item_done 中:
    ResponseItem::Reasoning { .. } => true,  // preempt for mailbox
    // ...
}

pub enum MessagePhase {
    Commentary,  // ← 推理/思考阶段标记
    // ...
}
```

### 4.4 Reasoning 追踪上下文

```rust
let reasoning_effort = turn_context.effective_reasoning_effort_for_tracing();

// OpenTelemetry 属性注入
otel.name = field::Empty,
codex.request.reasoning_effort = %reasoning_effort,
codex.usage.reasoning_output_tokens = field::Empty,
```

### 4.5 推理 token 使用统计

```rust
// 在 usage 追踪中包含 reasoning tokens
let analytics_fact = TurnResolvedConfigFact {
    reasoning_effort: turn_context.reasoning_effort,
    reasoning_summary: Some(turn_context.reasoning_summary),
    // ...
};
```

---

## 五、工具系统（Tool Dispatch）

### 5.1 ToolRouter：工具路由与注册

```rust
// core/src/tools/router.rs
pub struct ToolRouter {
    registry: ToolRegistry,                         // 完整工具注册表
    specs: Vec<ConfiguredToolSpec>,                 // 所有工具规范
    model_visible_specs: Vec<ToolSpec>,             // 模型可见的工具
    parallel_mcp_server_names: HashSet<String>,     // 并行 MCP 服务器
}

pub(crate) struct ToolRouterParams<'a> {
    pub(crate) mcp_tools: Option<HashMap<String, ToolInfo>>,
    pub(crate) deferred_mcp_tools: Option<HashMap<String, ToolInfo>>,
    pub(crate) unavailable_called_tools: Vec<ToolName>,
    pub(crate) parallel_mcp_server_names: HashSet<String>,
    pub(crate) discoverable_tools: Option<Vec<DiscoverableTool>>,
    pub(crate) dynamic_tools: &'a [DynamicToolSpec],
}
```

### 5.2 工具类型全景

```rust
// 支持的响应项类型
pub enum ResponseItem {
    Message { role, phase, .. },               // 常规消息
    Reasoning { .. },                           // 推理内容
    FunctionCall { name, arguments, .. },       // 函数调用
    FunctionCallOutput { .. },                  // 函数调用结果
    LocalShellCall { .. },                      // 本地 Shell 调用
    ToolSearchCall { params, .. },              // 工具搜索调用
    CustomToolCall { call_id, name, .. },       // 自定义工具(插件/MCP)
    CustomToolCallOutput { .. },                // 自定义工具结果
    WebSearchCall { .. },                       // 网页搜索
    ImageGenerationCall { .. },                 // 图片生成
    Compaction { .. },                          // 压缩事件
    ContextCompaction { .. },                   // 上下文压缩
    // ...
}
```

### 5.3 `handle_output_item_done()` —— 工具结果统一处理

```rust
// core/src/stream_events_utils.rs
pub(crate) async fn handle_output_item_done(
    ctx: &mut HandleOutputCtx,
    item: ResponseItem,
    previous: Option<TurnItem>,
) -> CodexResult<OutputResult> {
    // 1. 识别 item 类型
    match &item {
        FunctionCall { .. } | LocalShellCall { .. } | CustomToolCall { .. } =>
            // → 创建异步 tool_future → push to in_flight
            // → needs_follow_up = true
        FunctionCallOutput { .. } | CustomToolCallOutput { .. } =>
            // → 记录工具结果 → needs_follow_up depends
        Message { role, .. } if role == "assistant" =>
            // → 提取 last_agent_message → needs_follow_up = false
        _ => {}
    }
}
```

---

## 六、并发工具执行

### 6.1 FuturesOrdered 异步编排

```rust
// 并发工具调用容器
let mut in_flight = FuturesOrdered::new();

// 工具执行以 BoxFuture 推入并发队列
if let Some(tool_future) = output_result.tool_future {
    in_flight.push_back(tool_future);
}

// 在循环末尾 await 所有 in_flight futures
while let Some(result) = in_flight.next().await {
    // 收集工具执行结果 → 推入 session
    sess.record_conversation_items(&turn_context, &[result_item]).await;
}
```

**特性**：`FuturesOrdered` 保证**按推入顺序产生结果**，即使异步任务实际执行可以并行。

### 6.2 ToolCallRuntime 与 ParallelToolCalls

```rust
// Protocol 级别的并行控制
struct Prompt {
    input: Vec<ResponseItem>,
    tools: Vec<ToolSpec>,
    parallel_tool_calls: bool,   // ← 模型是否支持并行工具调用
    base_instructions: BaseInstructions,
    personality: Option<String>,
    output_schema: Option<serde_json::Value>,
}

// ToolCallRuntime 持有 router + session + turn_context + diff_tracker
let tool_runtime = ToolCallRuntime::new(
    Arc::clone(&router),
    Arc::clone(&sess),
    Arc::clone(&turn_context),
    Arc::clone(&turn_diff_tracker),
);
```

---

## 七、上下文压缩

### 7.1 压缩触发条件

codex-rs 的压缩在**两个时机**触发：

```
1. Pre-sampling compaction (采样前)
   total_usage_tokens >= auto_compact_limit → run_auto_compact(phase=PreTurn)

2. Mid-turn compaction (循环中)
   token_limit_reached && needs_follow_up → run_auto_compact(phase=MidTurn)
```

### 7.2 压缩分支：本地 vs 远程

```rust
// core/src/session/turn.rs - 第 804 行
async fn run_auto_compact(
    sess, turn_context, client_session,
    initial_context_injection, reason, phase,
) -> CodexResult<bool> {
    if should_use_remote_compact_task(turn_context.provider.info()) {
        if features.enabled(Feature::RemoteCompactionV2) {
            run_inline_remote_auto_compact_task_v2(...)  // ← 远程压缩 v2
        } else {
            run_inline_remote_auto_compact_task(...)     // ← 远程压缩 v1
        }
    } else {
        run_inline_auto_compact_task(...)  // ← 本地压缩
    }
}
```

### 7.3 压缩后不影响 Client Session

```rust
if reset_client_session {
    client_session.reset_websocket_session();  // 重置 WS 会话但保留路由
}
```

---

## 八、提示词工程

### 8.1 Prompt 结构体

```rust
// core/src/session/turn.rs - 第 973 行
pub(crate) fn build_prompt(
    input: Vec<ResponseItem>,       // 对话历史（用户+助手+工具结果）
    router: &ToolRouter,            // 工具路由（含 model_visible_specs）
    turn_context: &TurnContext,     // turn 上下文（配置/模型/权限）
    base_instructions: BaseInstructions,  // 基础系统指令
) -> Prompt {
    Prompt {
        input,                                           // 对话输入
        tools: router.model_visible_specs(),              // 模型可见工具
        parallel_tool_calls: supports,                    // 并行调用支持
        base_instructions,                                // 系统指令
        personality: turn_context.personality,            // Agent 人格
        output_schema: final_output_json_schema,          // 输出 JSON Schema
        output_schema_strict: !is_guardian_reviewer,     // 严格模式
    }
}
```

### 8.2 BaseInstructions 的来源

```rust
// 从 Session 获取基础指令
let base_instructions = sess.get_base_instructions().await;

// BaseInstructions 包含:
// - System Prompt（你是Codex...）
// - 可用能力描述
// - 安全约束
// - 环境信息
```

### 8.3 Personality 注入

```rust
pub struct TurnContext {
    pub personality: Option<String>,  // "helpful", "precise", "creative" 等
    // ...
}
```

---

## 九、权限与安全

### 9.1 三层 Hook 模型

```
用户输入 → UserPromptSubmit Hook (第一道: 可中断)
    │
    ▼
██ ReAct Loop ████████████████████████████████
    │
    ├─ SessionStart Hook (每次 continue 前)
    │
    ├─ Sampling → LLM 调用
    │     └─ 响应事件流
    │         ├─ Reasoning → 不执行
    │         ├─ ── → 不执行 (Mailbox preempt)
    │         └─ tool_call → handle_output_item_done
    │              ├─ PermissionPolicy.authorize()
    │              │   AskForApproval::Never → 自动允许
    │              │   AskForApproval::Granular → 细粒度策略
    │              │   AskForApproval::OnRequest → 用户确认
    │              │
    │              └─ tool_execution → PostToolUse Hook
    │
    └─ 停止条件满足 → Stop Hook → AfterAgent Hook
```

### 9.2 五种审批模式

```rust
pub enum AskForApproval {
    Never,          // 永不询问（开发环境）
    UnlessTrusted,  // 可信目录除外
    OnFailure,      // 仅工具失败时确认
    OnRequest,      // 工具请求时确认
    Granular(..),   // 细粒度策略（命令+文件+网络）
}
```

### 9.3 PendingInput Hook

这是一个独特设计：用户在 Agent 运行期间通过 UI 发送的消息，需要**先经过 Hook 审查**才能注入到对话流：

```rust
match inspect_pending_input(&sess, &turn_context, pending_input_item).await {
    PendingInputHookDisposition::Accepted(pending_input) =>
        record_pending_input(...).await,    // 注入对话
    PendingInputHookDisposition::Blocked { additional_contexts } =>
        // 阻止此消息，且有后续消息 → requeue
        // 无后续消息且无accepted → break
}
```

---

## 十、消息事件总线

### 10.1 `EventMsg` 枚举 —— 全事件类型

```rust
pub enum EventMsg {
    // Agent 消息
    AgentMessageContentDelta(AgentMessageContentDeltaEvent),
    AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent),
    AgentReasoningRawContent(ReasoningRawContentDeltaEvent),

    // CoT / Reasoning
    ReasoningContentDelta(ReasoningContentDeltaEvent),
    ReasoningRawContentDelta(ReasoningRawContentDeltaEvent),

    // 计划
    PlanDelta(PlanDeltaEvent),

    // Turn diff
    TurnDiff(TurnDiffEvent),

    // 用户交互
    AskForApproval(...),     // 权限请求
    RequestUserInput(...),   // 用户输入请求

    // 状态
    HookStarted(...),        // Hook 启动
    HookCompleted(...),      // Hook 完成
    Warning(WarningEvent),   // 警告
    Error(ErrorEvent),       // 错误
}
```

### 10.2 `ResponseItem` → `TurnItem` 转换

```rust
fn parse_turn_item(item: &ResponseItem) -> Option<TurnItem> {
    match item {
        ResponseItem::Message { role, content, .. } => {
            TurnItem::AgentMessage(/*...*/)  // 或 UserMessage
        }
        ResponseItem::FunctionCall { .. } => TurnItem::FunctionCall(/*...*/),
        ResponseItem::Reasoning { .. } => TurnItem::Reasoning(/*...*/),
        // ... 每种 ResponseItem 映射到对应的 TurnItem
    }
}
```

### 10.3 Session → UI 事件通道

```rust
// 通过  sess.send_event(&turn_context, event)  发送事件到UI
sess.send_event(&turn_context, EventMsg::Warning(event)).await;

// 底层通过 WebSocket unix socket 或 HTTP SSE 推送
```

---

## 十一、Session 生命周期

```
┌────────────────────────────────────────────────────────┐
│                    Session 生命周期                      │
│                                                         │
│  1. boot() → 初始化存储/认证/resolver                   │
│  2. thread_start() → 创建 TurnContext + 预配置          │
│  3. turn_start() → run_turn()  ReAct 循环              │
│  4. turn_steer() → 中间注入消息（pending_input）       │
│  5. compact() → 上下文压缩                              │
│  6. fork() → 会话分支（独立 state）                    │
│  7. archive/unarchive → 归档/取消归档                   │
│  8. rollback() → 回滚到 checkpoint                     │
└────────────────────────────────────────────────────────┘
```

---

## 十二、整体数据流总结

```
┌──────────────────────────────────────────────────────────────┐
│                 codex-app-server (HTTP/WS)                    │
│                                                               │
│  POST /v2/thread/start → boot → Session 初始化               │
│  POST /v2/turn/start   → run_turn(input)                     │
│                             │                                 │
│    ┌────────────────────────┼─────────────────────┐          │
│    │               codex-core (Agent Engine)      │          │
│    │                                              │          │
│    │  run_turn(session, turn_ctx, input, cancel)  │          │
│    │    │                                         │          │
│    │    ├─ pre-sampling compact                   │          │
│    │    ├─ skill/plugin injections                │          │
│    │    ├─ session-start hooks                    │          │
│    │    ├─ user-prompt-submit hooks               │          │
│    │    │                                         │          │
│    │    └─ loop {  ←── ReAct 核心循环             │          │
│    │         ├─ session-start hooks (每轮检查)    │          │
│    │         ├─ drain pending_input               │          │
│    │         ├─ run_sampling_request()            │          │
│    │         │   ├─ built_tools() → ToolRouter    │          │
│    │         │   ├─ build_prompt()                │          │
│    │         │   └─ try_run_sampling_request()    │          │
│    │         │       ├─ stream() → WS/HTTPS       │          │
│    │         │       ├─ ── ... delta              │          │
│    │         │       ├─ reasoning delta           │← CoT     │
│    │         │       ├─ function_call             │          │
│    │         │       └─ handle_output_item_done   │          │
│    │         │                                     │          │
│    │         ├─ token check → auto_compact?       │          │
│    │         ├─ no tool_uses? → stop/after hook   │          │
│    │         ├─ has pending? → continue           │          │
│    │         └─ done? → break                     │          │
│    │       }                                       │          │
│    │                                              │          │
│    │  返回 last_agent_message                      │          │
│    └──────────────────────────────────────────────┘          │
│                                                               │
│  事件流通过  sess.send_event()  →  UI  (WebSocket unix)     │
│  分析流通过  analytics_events_client                           │
│  追踪流通过  OpenTelemetry spans                               │
└──────────────────────────────────────────────────────────────┘
```

---

## 十三、四个项目全面对比

| 特性 | cc-haha (TS) | claude-code-rust (Rust) | claw-code (Rust) | **codex-rs (Rust)** |
|------|-------------|------------------------|------------------|---------------------|
| **ReAct 循环** | ✅ while(true) + 流式工具 | ❌ 单次调用 | ✅ loop + tool迭代 | ✅ loop + pending_input + 多层Hook |
| **CoT/Thinking** | ✅ Extended Thinking (3模式) | ❌ | ✅ API层 thinking blocks | ✅ reasoning_effort + summary + raw/摘要delta |
| **并发工具执行** | ✅ StreamingToolExecutor | ❌ | ✅ tokio async | ✅ FuturesOrdered + ToolCallRuntime |
| **上下文压缩** | ✅ 6层 | ✅ ContextWindow | ✅ Auto+Manual+Health | ✅ pre/mid-turn + remote v2 + local |
| **权限控制** | ✅ | ❌ | ✅ Policy+3Hook | ✅ AskForApproval(5模式) + PendingInput Hook |
| **传输层** | SSE | blocking HTTP | SSE | ✅ WebSocket → HTTPS fallback + retry |
| **多 Agent** | ✅ 子代理+Coordinator | ✅ 5内置Agent | ❌ | ✅ Mailbox + CodeMode + AppConnectors |
| **可观测性** | ✅ | ❌ | ✅ SessionTracer | ✅ OpenTelemetry + Analytics + SessionTelemetry |
| **沙箱** | ❌ | ❌ | ❌ | ✅ Windows Sandbox + Linux Sandbox |
| **插件生态** | ❌ | ✅ PluginMarketplace | ❌ | ✅ PluginsManager + MCP App体系 |
| **规模** | 中等 | 小型 (骨架) | 中等 | 🔴 超大型 (90+ crate) |

---

## 十四、关键启示（对 Hclaw 项目的参考价值）

### 14.1 架构上的最佳实践

codex-rs 的多层架构展示了 Rust 大型项目的黄金标准：

```
core（纯逻辑，无IO）      →  Agent Loop + Tool Router + Session
  ↑ 依赖
protocol（纯数据）         →  ResponseItem / EventMsg / TurnItem
  ↑ 实现
codex-api（IO操作）         →  OpenAI API Client + WebSocket
app-server（服务层）        →  HTTP API + WebSocket Server
```

**关键点**：`core` 不直接依赖 HTTP/Ws，只通过 `ModelClientSession` trait 抽象。这意味着：
- `core` 可以纯单元测试（mock client session）
- 可以轻松切换到其他 API 提供商
- 前后端通信协议与核心引擎完全解耦

### 14.2 Agent Loop 设计精要

```rust
// codex-rs 的 Agent Loop 模式可直接应用于 Hclaw
pub struct HclawRuntime {
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    client: ModelClientSession,     // ← trait 抽象
    tool_router: Arc<ToolRouter>,
    max_iterations: usize,
}

impl HclawRuntime {
    pub async fn run_turn(&mut self, input: UserInput) -> Option<String> {
        self.session.push_user(input);

        loop {
            // 1. build prompt → tools + history + personality
            let prompt = self.build_prompt();

            // 2. Thought + Action: stream LLM API
            let stream = self.client.stream(prompt, reasoning_effort);
            let mut needs_follow_up = false;

            // 3. handle events (text delta / reasoning delta / tool_uses)
            while let Some(event) = stream.next().await {
                match event {
                    TextDelta(text) → self.session.push_assistant_text(text),
                    ReasoningDelta(text) → self.session.push_reasoning(text),
                    ToolUse { id, name, input } → {
                        let result = self.tool_router.execute(&name, &input).await;
                        self.session.push_tool_result(id, name, result);
                        needs_follow_up = true;
                    }
                    OutputDone → { /* save message */ }
                    MessageStop → break,
                }
            }

            // 4. no tool uses? → done
            if !needs_follow_up { break; }
            // → continue ReAct loop
        }
        self.session.last_agent_message()
    }
}
```

### 14.3 值得直接借鉴的模块

1. **`TurnContext` + `Session` 双 Arc 模式**：Turn 是短命的（每轮一次），Session 是长命的（跨多轮），通过 Arc 共享
2. **`FuturesOrdered` 并发工具**：保留顺序，内部异步
3. **`EventMsg` 统一事件总线**：所有 UI 通信走同一个枚举，前端只需 switch
4. **`AskForApproval` 审批模式**：5 级从"永不询问"到"细粒度控制"
5. **WebSocket → HTTPS fallback**：自动降级，用户无感知

