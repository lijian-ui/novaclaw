# Agent 工具调用优化方案

> 日期：2026-05-09
> 相关讨论：关于 ReAct 循环中工具定义（tools）传递策略的优化思考

---

## 1. 当前做法与问题

### 1.1 现状

目前在 `backend/src/agent/runtime.rs` 的 `call_llm_with_tools` 中，**每轮对话**都把所有注册的工具定义通过 `tools` 参数传给 LLM：

```rust
// 构建 LLM 工具 Schema
let llm_tools: Vec<crate::llm::types::ToolDef> = tools
    .iter()
    .map(|t| crate::llm::types::ToolDef {
        def_type: "function".to_string(),
        function: FunctionDef {
            name: t.function.name.clone(),
            description: t.function.description.clone(),
            parameters: t.function.parameters.clone(),
        },
    })
    .collect();

// 随聊天请求一起发给 LLM
let request = ChatRequest {
    messages,
    tools: Some(llm_tools),  // 每轮都传
    // ...
};
```

### 1.2 问题

- **Token 浪费**：一个工具定义约 100-300 tokens，注册 15 个工具就是 **1500-4500 tokens × 每轮调用**。
- **5 轮对话**仅工具定义就消耗 **7500-22500 tokens**。
- **很多对话根本不需要工具**（纯聊天场景），纯属浪费。
- 当前传入的所有工具定义参考：
  - `def_type: "function"`
  - `name`（工具名）
  - `description`（工具描述，通常较长）
  - `parameters`（JSON Schema，可能很复杂）

---

## 2. 优化方案

### 2.1 方案 A：文本暗示 + 重新调用

**核心思想**：第 1 次调用不带 tools，如果模型文本暗示需要工具，再重新带 tools 调用一次。

```
Phase 1: 不带 tools，纯聊天
         ↓
模型返回纯文本回复 → 代码解析是否提到工具名
         ↓ (提到工具)
Phase 2: 带上 tools，重新调用 LLM → 模型输出 tool_calls
         ↓
执行工具 → 结果送回 LLM → 最终回复
```

| 维度 | 评估 |
|------|------|
| Token 节省 | 50-80%（非工具对话完全不传） |
| 可靠性 | **低** — 文本解析不可靠，模型可能暗示但不精确 |
| 通用性 | 所有模型都支持 |
| 额外延迟 | 多 1 轮 LLM 调用 |
| 实现难度 | 中等 — 需要可靠的意图解析逻辑 |

---

### 2.2 方案 B：注册"查询工具"工具（渐进式发现）

**核心思想**：只注册一个最小的元工具 `get_available_tools`，让模型主动查询可用工具。

```
注册:
  tools = [get_available_tools]  // 只传这一个

第 1 轮模型调用 get_available_tools
  → 返回所有工具描述（read_file, write_file, ...）
  → LLM 看到工具有哪些

第 2 轮：把 read_file 等工具定义传给 LLM
  → 模型调用 read_file
```

| 维度 | 评估 |
|------|------|
| Token 节省 | 80-90%（首轮只传 1 个工具） |
| 可靠性 | **中** — 取决于模型是否主动调用查询工具 |
| 通用性 | 所有模型都支持 |
| 额外延迟 | 多 1-2 轮 LLM 调用 |
| 实现难度 | 中等 — 需要注册元工具并处理二级调用 |

---

### 2.3 方案 C：`tool_choice` 参数控制

**核心思想**：利用 LLM API 原生的 `tool_choice` 参数控制工具行为。

```
第 1 次调用：
  tools = [所有工具]
  tool_choice = "none"    → 强制不调工具，纯文本回复（思考过程）

第 2 次调用：
  tools = [所有工具]
  tool_choice = "auto"    → 模型正常判断是否调工具
```

**`tool_choice` 参数的含义：**

| 值 | 行为 |
|----|------|
| `"none"` | 强制不调工具，即使 tools 参数有值 |
| `"auto"` | 模型自主决定是否调工具（默认） |
| `"required"` | 强制必须调用工具 |
| `{"type":"function","function":{"name":"xxx"}}` | 强制调用指定工具 |

| 维度 | 评估 |
|------|------|
| Token 节省 | 50-80%（非工具对话少传一轮） |
| 可靠性 | **高** — API 原生语义保证 |
| 通用性 | **仅部分 API/模型支持**（OpenAI、DeepSeek 支持，部分本地模型不支持） |
| 额外延迟 | 多 1 轮 LLM 调用 |
| 实现难度 | 低 — 只需改参数 |

**支持 `tool_choice` 的常见模型/平台：**

| 平台/模型 | 支持情况 |
|-----------|---------|
| OpenAI (GPT-4o, GPT-4-turbo) | ✅ 完全支持 |
| DeepSeek (deepseek-chat) | ✅ 支持 |
| Claude (Anthropic) | ❌ 不支持（使用 `tool_choice` 字段但语法不同） |
| 本地模型 (Ollama/LM Studio) | ❌ 通常不支持 |
| OpenRouter | 取决于上游模型 |

---

## 3. 方案对比总结

| 维度 | 当前做法 | 方案 A：文本暗示 | 方案 B：查询工具 | 方案 C：tool_choice |
|------|---------|----------------|----------------|-------------------|
| **Token 节省** | 0% | 50-80% | 80-90% | 50-80% |
| **可靠性** | ✅ 高 | ⚠️ 低 | 🟡 中 | ✅ 高 |
| **通用性** | ✅ 所有模型 | ✅ 所有模型 | ✅ 所有模型 | ⚠️ 部分模型 |
| **额外延迟** | 无 | +1 轮 | +1-2 轮 | +1 轮 |
| **实现难度** | - | 中 | 中 | 低 |
| **代码改动量** | - | 中等 | 中等 | 小 |

---

## 4. 推荐方案

### 混合策略（推荐）

**优先使用方案 C**（`tool_choice`），如果不支持则降级到方案 A：

```
1. 检查模型是否支持 tool_choice 参数
   ├── 支持 → 使用方案 C：
   │   第 1 轮: tool_choice = "none", 有 tools
   │   第 2 轮: tool_choice = "auto", 有 tools
   │
   └── 不支持 → 使用方案 A 变种：
       第 1 轮: 不带 tools
       第 2 轮（如需要）: 带 tools
```

### 判断是否需要第 2 轮的策略

- **方案 C**：第 1 轮回复中如果模型提到需要工具操作，或第 1 轮的 reasoning 暗示要执行操作 → 触发第 2 轮
- **方案 A**：第 1 轮不带 tools 的回复文本中，如果包含已注册工具的关键词匹配 → 触发第 2 轮

---

## 5. 需要讨论的问题

1. **方案 A 的可靠性**：纯文本解析判断模型是否"想要"调用工具，准确率能到多少？是否有更好的启发式方法？
2. **方案 B 的实用性**：多一轮"查工具"的交互，用户体验上是否能接受？
3. **方案 C 的后备方案**：对于不支持 `tool_choice` 的模型（如 Claude、本地模型），用什么策略兜底？
4. **缓存策略**：工具定义是否可以在会话开始前先发给 LLM 做"预热"（预填充），减少每轮上下文重复？
5. **动态工具选择**：是否可以根据用户输入的关键词，只传相关的工具（如用户说"查文件"只传 read_file、list_files），不传不相关的（如 terminal commands）？

---

## 附录：相关代码位置

| 文件 | 关键函数/行 | 说明 |
|------|------------|------|
| `backend/src/agent/runtime.rs` | `call_llm_with_tools()` (L289-L487) | 当前工具传递的核心逻辑 |
| `backend/src/agent/runtime.rs` | `run_turn()` (L74-L286) | ReAct 主循环 |
| `backend/src/agent/cot.rs` | `CotExtractor::extract_multiple()` | CoT 思考提取 |
| `backend/src/agent/session.rs` | `AgentMessage` / `AgentSession` | 消息结构定义 |
| `backend/src/server/ws/chat.rs` | ws 处理 (L195-L224) | 消息持久化到 jsonl |
| `backend/src/server/routes/chat.rs` | `chat()` (L100-L131) | 非流式消息持久化 |
