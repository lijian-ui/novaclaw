# 记忆系统

jeeves 的记忆系统分为两层：**跨会话持久记忆（MEMORY.md）和**会话内历史搜索*（JSONL）。
---

## 0. 核心概念

| | 消息记录 (Transcript) | 记忆 (Memory) |
|---|---|---|
| **是什么 | 对话的完整日志，"谁说了什么 | 从中提炼的持久化知识 |
| **存什么 | 原始消息(role + content + tool_calls) | 事实、偏好、决策、环境知识|
| **格式** | JSONL (`sessions/<id>.jsonl`) | 纯文本(`MEMORY.md`)，`§` 分隔 |
| **检索* | 按时间顺序读取| 关键词匹配+ LLM 自行判断 |
| **例子** | `{"role":"user","content":"我喜欢简洁的回复"}` | `"User prefers concise responses"` |

---

## 1. 持久记忆 ：MEMORY.md

### 1.1 存储格式

`MEMORY.md` 存储在`memories/` 子目录下（`~/Documents/jeeves/memories/MEMORY.md`），每条记忆以`§` 分隔）
```
User prefers concise responses with bullet points

§ User runs Windows 11, uses VS Code

§ Project convention: snake_case for Rust, camelCase for TypeScript
```

### 1.2 存什么
| 类型 | 示例 | 是否存入 MEMORY.md |
|------|------|:---:|
| 用户偏好 | "我喜欢用 Rust" | 是|
| 项目约定 | "项目使用 tokio 异步运行时 | 是|
| 环境信息 | "用户使用 Windows 11" | 是|
| 任务进度 | "正在重构 login 模块" | →→session_search |
| Commit hash | "修复 #42 bug" | →→session_search |

### 1.3 写入时机

```
通道 A（即时）：LLM 调用 memory 工具 →追加到MEMORY.md
  触发条件：用户说 "记住/保存" / 用户分享偏好 / LLM 发现约定

通道 B（兜底）：上下文压缩时提取  触发条件：消息数超过 compact_threshold（默认40 条）
  动作：LLM 生成结构化摘要→摘要中的持久事实同步到MEMORY.md
```

### 1.4 注入到System Prompt

Session 启动时，MEMORY.md 内容作为 `## Persistent Memory` 段注入system prompt→
```text
## Memory Instructions

When the user shares preferences, project details, or personal information,
use the `memory` tool (action: add) to save them.

## Persistent Memory

Saved facts from previous sessions:

User prefers concise responses with bullet points
§ Project convention: snake_case for Rust
§ User runs Windows 11

Use these to personalize your responses.
```

- 注入时机：每次session 创建时（首次构建 system prompt→- 容量限制：截断到约3500 字符（~1000 tokens→- 不可变原则：session 内的写入不会更新 system prompt，下次session 才生效
---

## 2. `memory` 工具

### 2.1 工具描述

```json
{
  "name": "memory",
  "description": "Save and search persistent facts...",
  "parameters": {
    "action": {
      "type": "string",
      "enum": ["add", "search", "replace", "remove"]
    },
    "content": {
      "type": "string",
      "description": "The fact text (for add/replace/remove)"
    },
    "query": {
      "type": "string",
      "description": "Search keyword (for search action)"
    },
    "category": {
      "type": "string",
      "description": "Optional category label"
    }
  },
  "required": ["action"]
}
```

### 2.2 Actions

| action | 作用 | 必备参数 | 示例 |
|--------|------|---------|------|
| **add** | 保存新事实| `content` | `memory(action=add, content="用户喜欢 Rust")` |
| **search** | 搜已有记是| `query` | `memory(action=search, query="编程语言")` |
| **replace** | 更新旧事实| `content` 格式 `"旧内容\|新内容` | `memory(action=replace, content="Rust\|Go")` |
| **remove** | 删除匹配内容 | `content` | `memory(action=remove, content="喜欢 Go")` |

### 2.3 搜索策略

`search` 采用两步策略：
1. **精确匹配**：全文grep 关键词（大小写不敏感）2. **LLM 兜底**：如果精确匹配0 条且条目数≤20→*返回全部记忆，LLM 自己判断相关性）*

这样不需要向量搜索，LLM 自身就是最好的语义匹配器。
---

## 3. `session_search` 工具

### 3.1 工具描述

```json
{
  "name": "session_search",
  "description": "Search current session's JSONL transcript for temporal info...",
  "parameters": {
    "query": { "type": "string" },
    "limit": { "type": "integer" }
  },
  "required": ["query"]
}
```

### 3.2 用途
搜索当前会话内JSONL 历史，针→*临时性信→*（任务进度、bug 细节、commit 记录）。与 `memory/search` 的区别：

| | memory(search) | session_search |
|---|---|---|
| 搜索范围 | 跨会话(MEMORY.md) | 当前会话 (JSONL) |
| 适合场景 | 用户偏好、约定| 任务进度、技术细是|
| 数据类型 | 持久事实 | 临时信息 |

---

## 4. 系统架构

```
消息记录 (JSONL （完整对话日志)
       →       →LLM 提炼（上下文压缩时）
持久记忆 (MEMORY.md →跨会话持久知→
       →       →System prompt 注入（Session 启动时）
会话执行 (AgentRuntime)
  ├── memory tool → MEMORY.md
  ├── session_search →→JSONL
  └── compaction prompt →结构化摘→```

### 4.1 存储目录

```
{data_dir}/
├── memories/
→  └── MEMORY.md                  →跨会话持久记忆（纯文本，§ 分隔）
└── sessions/
    ├── messages/{id}.jsonl        → 消息记录（含 compaction 消息→    
    └── images/{sid}/              →图片实体
```

### 4.2 后端文件

| 文件 | 职责 |
|------|------|
| `memory/store.rs` | MEMORY.md 读写（add/replace/remove/search/list是|
| `tools/builtin.rs` | `memory` + `session_search` 工具注册 |
| `agent/prompt.rs` | SystemPromptBuilder →`build_memory()` 注入 |
| `agent/runtime.rs` | `build_system_prompt()` 加载 MEMORY.md；压缩时结构化摘是|

### 4.3 前端（不涉及→
记忆系统全部→LLM 层完成：LLM 调用工具读写 →写入 MEMORY.md →下次 session 自动注入。前端无需干涉→
---

## 5. 常见问题

**Q: 为什么不用向量搜索？**

当前 MEMORY.md 条目数极少（几十条），`search` 的精确匹配+ LLM 兜底策略已经足够。且记忆内容已注入system prompt，LLM 每次都看得到→
**Q: 记忆文件被手动修改了会怎样→*

下次 session 启动时重新读→system prompt。手动修改的条目立即生效（但不影响当前正在运行的 session）→
**Q: list 为什么不在工→schema 里？**

`list` →LLM 基本无用——记忆内容已→system prompt 里。handler 内部保留→`list`，调试时可直接调 API→