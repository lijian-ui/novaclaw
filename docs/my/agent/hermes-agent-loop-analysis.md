# Hermes Agent Agent 机制深度解读

> ReAct + Agent Loop + CoT 完整实现分析（Python 实现）

---

## 一、项目概览

**Hermes Agent** 是一个基于 Python 的 AI Agent 框架，使用 OpenAI 兼容的 API 协议，支持**工具调用循环、多模型推理、记忆管理和技能系统**。它是 NovaClaw/Hclaw 项目的主要参考上游。

### 核心特色

- 🐍 **纯 Python 实现**：asyncio 异步架构 + ThreadPoolExecutor 同步桥接
- 🔌 **OpenAI 兼容协议**：支持任何返回 ChatCompletion 的 API（OpenAI、vLLM、SGLang、OpenRouter、Claude 等）
- 🔧 **模块化工具系统**：自注册工具注册表 + 工具集过滤 + 工具结果 Budget 控制
- 🧠 **多提供商 CoT**：统一提取多种 reasoning 格式（`reasoning` / `reasoning_content` / `reasoning_details` / 内联 `  thinking` 标签）
- 💾 **会话持久化**：SQLite 会话存储 + JSONL 轨迹导出 + 跨回合 Todo Store
- 📡 **中断式 API 调用**：后台线程 API 调用 + 主线程中断检测 → 用户可随时取消

---

## 二、整体架构

```
run_agent.py (AIAgent - 9000+ 行核心引擎)
    │
    ├─ run_conversation(user_message) → 主入口
    │     │
    │     ├─ _build_system_prompt() → 系统提示词组装
    │     │     ├─ prompt_builder.py → 身份/平台/技能索引/上下文文件
    │     │     ├─ memory_manager.py → 记忆检索 + 预取
    │     │     └─ context_engine.py → Token 使用追踪
    │     │
    │     └─ 主循环 while api_call_count < max_iterations:
    │          │
    │          ├─ step_callback(iteration, prev_tools)
    │          ├─ _drain_pending_steer() → UI中途注入
    │          ├─ _interruptible_api_call() → 后台线程API调用
    │          │     └─ _extract_reasoning() → CoT提取
    │          ├─ if tool_calls → handle_function_call() → 工具执行
    │          ├─ if no tool_calls → finish → return
    │          ├─ budget check → enforce_turn_budget()
    │          └─ compact? → context_engine.compress()
    │
    └─ 返回: { "finish_reason", "content", "messages", "usage", ... }
```

### 两套 Agent Loop 实现

项目包含**两套** Agent Loop：

| 实现 | 位置 | 用途 |
|------|------|------|
| `AIAgent.run_conversation()` | `run_agent.py` | 完整的 CLI/Gateway 引擎（9000+ 行，含缓存、compaction、中断、重试） |
| `HermesAgentLoop.run()` | `environments/agent_loop.py` | 轻量 RL 环境引擎（530 行，纯 clean loop） |

下面以 `HermesAgentLoop` 为主线 + `AIAgent` 为补充，分析核心机制。

---

## 三、ReAct 循环详解

### 3.1 核心入口

```python
# environments/agent_loop.py - 第 175 行
async def run(self, messages: List[Dict[str, Any]]) -> AgentResult:
    """
    执行完整的 Agent 循环，使用标准 OpenAI tool calling。

    循环模式：
    - 传入 tools= 到 API
    - 检查 response.choices[0].message.tool_calls
    - 通过 handle_function_call() 分派工具
    - 将 tool_result 追加到 messages
    - 继续循环直到模型不再调用工具
    """
```

### 3.2 HermesAgentLoop 的简洁 ReAct 循环

```python
for turn in range(self.max_turns):
    # ===== 1. Thought + Action: 调用 LLM API =====
    chat_kwargs = {
        "messages": messages,
        "tools": self.tool_schemas,        # ← 传入工具定义
        "temperature": self.temperature,
    }
    response = await self.server.chat_completion(**chat_kwargs)
    assistant_msg = response.choices[0].message

    # ===== CoT: 提取推理内容 =====
    reasoning = _extract_reasoning_from_message(assistant_msg)
    reasoning_per_turn.append(reasoning)

    # ===== 2. 检查是否有工具调用 =====
    if assistant_msg.tool_calls:
        # 保存 assistant 消息（含 tool_calls）
        messages.append({
            "role": "assistant",
            "content": assistant_msg.content or "",
            "tool_calls": [...],
            "reasoning_content": reasoning,       # ← 保留推理上下文
        })

        # ===== 3. Observation: 逐一执行工具 =====
        for tc in assistant_msg.tool_calls:
            tool_name = tc.function.name
            tool_args = json.loads(tc.function.arguments)

            # 工具调度
            tool_result = handle_function_call(tool_name, tool_args, task_id)

            # 工具结果持久化（Budget 控制）
            tool_result = maybe_persist_tool_result(
                content=tool_result, tool_name=tool_name, tool_use_id=tc_id,
                env=get_active_env(self.task_id), config=self.budget_config,
            )

            # 推入 tool 消息
            messages.append({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": tool_result,
            })

        # 工具结果 Budget 强制执行
        enforce_turn_budget(messages[-num_tcs:], ...)

    else:
        # ===== 4. 无工具调用 → 任务完成 =====
        messages.append({"role": "assistant", "content": ...})
        return AgentResult(
            messages=messages,
            turns_used=turn + 1,
            finished_naturally=True,
            reasoning_per_turn=reasoning_per_turn,
        )

# ===== 5. max_turns 达到 =====
return AgentResult(
    messages=messages,
    turns_used=self.max_turns,
    finished_naturally=False,
)
```

### 3.3 完整流程图

```
┌────────────────────────────────────────────────────────────┐
│            HermesAgentLoop.run(messages)                    │
│                                                             │
│  for turn in range(30):                                     │
│    │                                                        │
│    ├─ ▸ Thought+Action: server.chat_completion(tools, msgs)│
│    │                                                        │
│    ├─ ▸ CoT: _extract_reasoning_from_message()             │
│    │       ├─ reasoning_content                            │
│    │       ├─ reasoning                                    │
│    │       └─ reasoning_details[].text                     │
│    │                                                        │
│    ├─ ▸ 检测 tool_calls                                    │
│    │   ├─ YES → push assistant msg (含 reasoning_content)  │
│    │   │        for each tc:                               │
│    │   │          ├─ json.loads(args)                      │
│    │   │          ├─ handle_function_call(name, args)      │
│    │   │          ├─ maybe_persist_tool_result()           │
│    │   │          └─ push tool msg → messages              │
│    │   │        enforce_turn_budget()                      │
│    │   │        continue → 回到下一个 turn                  │
│    │   │                                                   │
│    │   └─ NO  → push assistant msg                        │
│    │            return AgentResult(finished_naturally=True) │
│    │                                                        │
│  超过 max_turns → return AgentResult(finished=False)       │
└────────────────────────────────────────────────────────────┘
```

### 3.4 AIAgent 的增强版循环

`AIAgent.run_conversation()` 在基础循环上增加了：

```
while api_call_count < max_iterations && iteration_budget.remaining > 0:
    ├─ _interrupt_requested? → break
    ├─ step_callback(turn, prev_tools)       ← Gateway hook注入
    ├─ _drain_pending_steer()                ← UI中途 /steer消息
    ├─ _sanitize_tool_call_arguments()        ← 工具参数corruption修复
    ├─ 组装 api_messages
    │   ├─ 注入 memory prefetch (到user msg)
    │   ├─ 注入 plugin pre_llm_call hooks
    │   ├─ _copy_reasoning_content_for_api() ← 跨turn保留reasoning
    │   ├─ apply_anthropic_cache_control()   ← Prompt缓存
    │   └─ 安全清理(strip orphaned tool results/unknown fields)
    ├─ _interruptible_api_call(api_kwargs)   ← 后台线程 + stale检测
    ├─ _extract_reasoning()                  ← 多provider CoT
    ├─ tool_calls? → handle_function_call() → tool results
    ├─ _check_compression() → maybe compress
    ├─ 各种重试/回退:
    │   ├─ invalid_tool_retry
    │   ├─ invalid_json_retry
    │   ├─ empty_content_retry
    │   ├─ incomplete_scratchpad_retry
    │   ├─ post_tool_empty_retry
    │   └─ provider fallback
    └─ _budget_grace_call → 最后一次机会
```

### 3.5 并发工具执行

```python
# 全局线程池（可运行时调整大小）
_tool_executor = concurrent.futures.ThreadPoolExecutor(max_workers=128)

# 工具在后台线程运行，避免阻塞事件循环
tool_result = await loop.run_in_executor(
    _tool_executor,
    lambda: handle_function_call(tool_name, args, task_id=task_id),
)

# 容忍慢工具：记录超过30s的工具 + 线程池队列深度
if tool_elapsed > 30:
    logger.warning("turn %d: %s took %.1fs (pool queue=%d)", ...)
```

---

## 四、CoT（Chain of Thought / Reasoning）实现

### 4.1 多提供商 Reasoning 统一提取

Hermes Agent 的核心创新之一是**对多种 reasoning 格式的统一抽象**：

```python
def _extract_reasoning(self, assistant_message) -> Optional[str]:
    """从 assistant message 中提取 reasoning/thinking 内容"""

    reasoning_parts = []

    # 1) message.reasoning 直接字段
    if hasattr(assistant_message, 'reasoning') and assistant_message.reasoning:
        reasoning_parts.append(assistant_message.reasoning)

    # 2) message.reasoning_content 替代名
    if hasattr(assistant_message, 'reasoning_content') and assistant_message.reasoning_content:
        if assistant_message.reasoning_content not in reasoning_parts:
            reasoning_parts.append(assistant_message.reasoning_content)

    # 3) message.reasoning_details[].text OpenRouter统一格式
    if hasattr(assistant_message, 'reasoning_details'):
        for detail in assistant_message.reasoning_details:
            if isinstance(detail, dict):
                summary = detail.get('summary') or detail.get('thinking') \
                       or detail.get('content') or detail.get('text')
                if summary and summary not in reasoning_parts:
                    reasoning_parts.append(summary)

    # 4) 内联  thinking / reasoning 标签（无结构化字段时的兜底）
    content = getattr(assistant_message, "content", None)
    if not reasoning_parts and isinstance(content, str) and content:
        inline_patterns = (
            r"  thinking(.*?)  response",
            # ...
        )
        # regex提取内联推理

    return "\n".join(reasoning_parts) if reasoning_parts else None
```

### 4.2 运行时 CoT 流程图

```
API Response
    │
    ├─ message.reasoning = "..."
    │      → reasoning_parts.append(...)
    │
    ├─ message.reasoning_content = "..."
    │      → reasoning_parts.append(...)  (dedup)
    │
    ├─ message.reasoning_details = [{type: "reasoning.summary", thinking: "..."}]
    │      → extract summary/thinking/content/text from each detail
    │
    └─ (no structured fields?)
         → parse <thinking>...</thinking> inline tags from message.content
         → regex:  thinking(.*?)  response
```

### 4.3 Reasoning 内容在对话中的生命周期

1. **API 响应时**：`_extract_reasoning()` 提取 → 存入 `msg["reasoning_content"]`
2. **下一轮 API 调用时**：`_copy_reasoning_content_for_api()` 将 `reasoning_content` 传回 API → 保持多轮推理上下文
3. **轨迹存储**：JSONL 文件中保存完整 reasoning
4. **回调通知**：`reasoning_callback(content)` + `thinking_callback(tokens)` → UI 实时显示 CoT

### 4.4 `_copy_reasoning_content_for_api()` —— 跨轮保留推理

```python
def _copy_reasoning_content_for_api(self, internal_msg, api_msg):
    """
    For ALL assistant messages, pass reasoning back to the API.
    This ensures multi-turn reasoning context is preserved.
    """
    if "reasoning_content" in internal_msg:
        api_msg["reasoning_content"] = internal_msg["reasoning_content"]
    if hasattr(internal_msg, "reasoning_details"):
        api_msg["reasoning_details"] = internal_msg["reasoning_details"]
```

### 4.5 `reasoning_config` —— CoT 行为配置

```python
class AIAgent:
    def __init__(self, reasoning_config=None, ...):
        """
        Args:
            reasoning_config (Dict): OpenRouter reasoning configuration
                e.g. {"effort": "none"} → 禁用 thinking
                     {"effort": "high", "summary": "auto"} → 最高推理
        """
```

---

## 五、工具系统

### 5.1 自注册工具注册表

```python
# tools/registry.py
registry = ToolRegistry()

# 每个工具文件在导入时自动注册
# tools/terminal_tool.py
from tools.registry import register

register(
    name="terminal",
    schema=TERMINAL_SCHEMA,
    handler=terminal_handler,
    metadata={"toolset": "core", "requirements": []},
)
```

### 5.2 工具集（Toolset）过滤

```python
# toolsets.py
_HERMES_CORE_TOOLS = [
    "terminal", "read", "write", "edit", "glob", "grep",
    "browser_snapshot", "memory", "web_search", "todo",
    "delegate_task", "task", "session_search",
    "skill_manage", "skill_use", "git", "mcp", ...
]

# AIAgent支持:
# - enabled_toolsets: 只启用特定工具集
# - disabled_toolsets: 禁用特定工具集
# - quiet_mode: 静默模式下的工具集扁平化
```

### 5.3 `handle_function_call()` —— 统一工具调度

```python
def handle_function_call(function_name, function_args, task_id, user_task) -> str:
    # 1. 查找注册的处理函数
    handler = registry.get_handler(function_name)
    if handler is None:
        return json.dumps({"error": f"Unknown tool: {function_name}"})

    # 2. 调用处理函数（支持同步和异步）
    result = handler(function_args, task_id=task_id, user_task=user_task)

    # 3. 返回 JSON 字符串（符合 OpenAI tool result 格式）
    return result
```

### 5.4 Tool Result Budget

```python
# tools/budget_config.py
@dataclass
class BudgetConfig:
    per_tool_thresholds: Dict[str, int]    # 每种工具的最大token
    per_turn_aggregate_budget: int         # 单turn总budget
    preview_size: int                      # 预览截断大小

# 工具结果持久化时检查：
tool_result = maybe_persist_tool_result(
    content=tool_result,
    tool_name=tool_name,
    tool_use_id=tc_id,
    env=get_active_env(task_id),
    config=self.budget_config,
)

# 整轮工具结果强制执行：
enforce_turn_budget(tool_messages, env=get_active_env(task_id), config=budget_config)
```

---

## 六、提示词工程

### 6.1 分层组装

```python
# AIAgent._build_system_prompt()
def _build_system_prompt(self):
    parts = []

    # 1. 身份声明（从 system_prompt.md）
    parts.append(self._system_prompt_template)

    # 2. 平台提示（OS / 环境信息）
    parts.append(prompt_builder.build_platform_hints())

    # 3. 技能索引（可用技能列表 + 触发条件）
    parts.append(prompt_builder.build_skill_index())

    # 4. 上下文文件（.cursorrules / AGENTS.md / SOUL.md）
    parts.append(prompt_builder.build_context_files())

    # 5. 记忆注入（MEMORY.md / USER.md + 跨会话预取）
    parts.append(self._memory_manager.build_memory_prompt())

    # 6. 临时系统提示（ephemeral，API-call-time only）
    # 注入位置：API call前，不持久化到 DB

    return "\n".join(parts)
```

### 6.2 临时/持久分离

```
┌─────────────────────────────┐
│ 持久提示（会话DB中存储）      │
│ ├─ 身份声明                  │
│ ├─ 平台提示                  │
│ ├─ 技能索引                  │
│ └─ 上下文文件               │
├─────────────────────────────┤
│ 临时提示（API call-time注入）│ ← 不影响缓存
│ ├─ memory_manager.prefetch   │
│ ├─ ephemeral_system_prompt   │
│ └─ plugin pre_llm_call hooks │
└─────────────────────────────┘
```

### 6.3 注入防护

```python
# prompt_builder.py - 上下文文件注入检测
_CONTEXT_THREAT_PATTERNS = [
    (r'ignore\s+(previous|all|above|prior)\s+instructions', "prompt_injection"),
    (r'do\s+not\s+tell\s+the\s+user', "deception_hide"),
    (r'system\s+prompt\s+override', "sys_prompt_override"),
    (r'disregard\s+(your|all|any)\s+(instructions|rules|guidelines)', "disregard_rules"),
    # ... 10 patterns
]

# 不可见字符检测
_CONTEXT_INVISIBLE_CHARS = {
    '\u200b', '\u200c', '\u200d', '\u2060', '\ufeff',   # Zero-width / BOM
    '\u202a', '\u202b', '\u202c', '\u202d', '\u202e',   # Bi-directional override
}

def _scan_context_content(content, filename):
    # 扫描 → return sanitized or "[BLOCKED: ...]"
```

---

## 七、上下文压缩

### 7.1 ContextEngine 抽象

```python
class ContextEngine(ABC):
    """可插拔上下文引擎基类"""

    @abstractmethod
    def update_from_response(self, usage: Dict) -> None: ...
    @abstractmethod
    def should_compress(self, prompt_tokens: int = None) -> bool: ...
    @abstractmethod
    def compress(self, messages, current_tokens) -> CompactionResult: ...
```

### 7.2 Compaction 触发

```python
# 默认 ContextCompressor
threshold_percent = 0.75      # token使用达75%时触发
protect_first_n = 3           # 保护前3条消息
protect_last_n = 6            # 保护后6条消息

# 压缩时:
# - 保留 [0..protect_first_n)  +  [len-protect_last_n..)
# - 中间部分生成摘要
# - 替换为 <summary>...</summary>
```

---

## 八、记忆系统

```python
# agent/memory_manager.py
class MemoryManager:
    def build_memory_prompt(self) -> str:
        """组装记忆上下文"""
        parts = []

        # 1. MEMORY.md / USER.md 文件内容
        parts.append(self._read_memory_files())

        # 2. SQLite FTS5 全文搜索历史
        parts.append(self._search_history(query))

        # 3. 跨会话知识图谱
        parts.append(self._knowledge_graph_context())

        return "\n".join(parts)

    def prefetch_all(self, query) -> str:
        """预取所有相关记忆（异步触发）"""
        # 后台预取，下次 user message 注入
```

---

## 九、中断机制

### 9.1 设计 —— 用户可以在任何时候取消

```python
# _interruptible_api_call
def _interruptible_api_call(self, api_kwargs):
    """
    在后台线程运行 API 调用，主循环可以检测中断信号
    而无需等待 HTTP 往返完成。

    每个 Worker 线程获得独立的 OpenAI 客户端实例。
    中断只关闭该 Worker 的客户端 → 重试时用新客户端。
    """
    result = {"response": None, "error": None}
    request_client_holder = {"client": None}

    # 在后台线程执行
    thread = threading.Thread(target=_execute_call, args=(...))
    thread.start()

    # 主线程轮询中断信号
    while thread.is_alive():
        if self._interrupt_requested:
            _cancel_call(request_client_holder["client"])
            break
        time.sleep(0.01)  # 10ms 轮询间隔
```

---

## 十、会话持久化

```python
# session_store.py
class SessionStore:
    """SQLite 会话存储"""

    def save_session(self, session_id, messages, metadata):
        """保存完整会话"""
        # 1. 插入到 sessions 表
        # 2. 批量插入到 messages 表
        # 3. 更新 FTS5 全文索引
```

### JSONL 轨迹

```python
# 可选轨迹导出
if self.save_trajectories:
    with open(trajectory_path, "a") as f:
        f.write(json.dumps({
            "turn": turn, "messages": messages,
            "reasoning": reasoning_per_turn,
            "api_call_count": api_call_count,
        }) + "\n")
```

---

## 十一、全部五个项目对比

| 特性 | cc-haha (TS) | codex-rs (Rust) | **Hermes Agent (Py)** |
|------|-------------|-----------------|----------------------|
| **语言** | TypeScript | Rust | Python |
| **ReAct 循环** | ✅ while(true) | ✅ loop + multi-Hook | ✅ for range(max_turns) |
| **CoT/Thinking** | ✅ Extended Thinking API | ✅ reasoning_effort + 3 delta | ✅ **4-level unified extraction** (reasoning/reasoning_content/reasoning_details/inline tags) |
| **多Provider CoT** | ❌ 仅Anthropic | ❌ 仅OpenAI | ✅ **Anthropic/OpenAI/DeepSeek/Qwen/Moonshot/OpenRouter — 全部统一** |
| **工具执行** | ✅ StreamingToolExecutor | ✅ FuturesOrdered | ✅ ThreadPool(128) + loop.run_in_executor |
| **上下文压缩** | ✅ 6层 | ✅ pre/mid-turn | ✅ ContextEngine抽象 + threshold 75% |
| **中断机制** | ✅ abort signals | ✅ CancellationToken | ✅ **后台线程 + 主线程轮询 (10ms)** |
| **会话持久化** | ✅ JSONL | ✅ SQLite | ✅ SQLite + JSONL轨迹 |
| **记忆系统** | ✅ | ❌ | ✅ MEMORY.md + FTS5 + prefetch |
| **注入防护** | ✅ | ❌ | ✅ **10 威胁模式 + 不可见字符检测** |
| **工具集过滤** | ❌ | ❌ | ✅ enabled_toolsets / disabled_toolsets |
| **ToolResult Budget** | ❌ | ❌ | ✅ per-tool + per-turn + preview |
| **规模** | 中 | 🔴超大(90+crate) | 🟡大(9000+行单文件) |

---

## 十二、关键启示（对 Hclaw 项目的参考价值）

### 12.1 Hermes Agent 的独特优势

1. **4-level unified CoT extraction** — 这是五个项目中**唯一**支持多提供商的统一推理提取层
2. **干净的两套 Loop** — `AIAgent` 是生产引擎，`HermesAgentLoop` 是干净的可复用组件 → Hclaw 应该只实现 1 套，但保留抽象
3. **自注册工具系统** — `@register()` 模式比 Trait 更零散但导入即自动激活

### 12.2 Hclaw 可以直接借鉴的模式

```rust
// Hclaw 的 CoT 提取层（借鉴 Hermes 的 4-level unified）
pub fn extract_reasoning(response: &ChatCompletion) -> Option<String> {
    let msg = &response.choices[0].message;

    // Level 1: reasoning_content (OpenRouter/DeepSeek)
    // Level 2: reasoning field (Qwen)
    // Level 3: reasoning_details (OpenRouter unified)
    // Level 4: inline <thinking> tags (fallback)

    let mut parts: Vec<String> = Vec::new();

    if let Some(ref r) = msg.reasoning_content { parts.push(r.clone()); }
    if let Some(ref r) = msg.reasoning { parts.push(r.clone()); }
    if let Some(ref details) = msg.reasoning_details {
        for detail in details {
            if let Some(text) = &detail.text { parts.push(text.clone()); }
        }
    }
    if parts.is_empty() {
        // fallback: parse <thinking>...</thinking> from content
        parts = extract_inline_thinking(&msg.content);
    }

    if parts.is_empty() { None } else { Some(parts.join("\n")) }
}
```

