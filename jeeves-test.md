好了，我已经把这个项目的里里外外摸透了，给你一份完整的分析和改造方案。

---

## 项目全景分析：Garudust Agent

### 一句话

这是一个**用 Rust 写的自改进 AI Agent 运行时**——不到 10MB 单二进制，无需运行时依赖，支持 TUI 聊天、7 个聊天平台接入、REST/WebSocket API、MCP 协议、技能系统。**没有遥测，没有锁定。**

### 架构纵览

```
garudust-agent/
├── bin/
│   ├── garudust/          → CLI 二进制（TUI + 子命令）
│   └── garudust-server/   → HTTP 服务器二进制
├── crates/
│   ├── garudust-core/     ★ 核心层：trait/类型/配置/错误定义
│   ├── garudust-agent/    ★ Agent 运行循环（think→tool→observe）
│   ├── garudust-transport/★ LLM 提供商（Anthropic/OpenAI/Ollama/Bedrock…）
│   ├── garudust-tools/    ★ 工具注册表 + MCP + 脚本工具 + 技能
│   ├── garudust-memory/   ★ 记忆存储（文件/SQLite/会话/文档）
│   ├── garudust-cron/     定时任务调度器
│   ├── garudust-gateway/   HTTP 网关（axum REST/SSE/WebSocket）
│   └── garudust-platforms/ 平台适配器（Telegram/Discord/Slack…）
```

### 数据流

```
用户输入
  → Agent::run() [garudust-agent]
    → build_system_prompt() (注入 skill/memory/tool schemas)
    → LLM 调用 [garudust-transport]
    → 解析工具调用
    → ToolRegistry.dispatch() [garudust-tools]
    → 结果回写对话历史
    → 重复直到 stop_reason = EndTurn
  → 输出到用户
```

### 当前有两种运行模式

| 模式 | 启动方式 | 界面 | 功能 |
|------|---------|------|------|
| **CLI/TUI** | `garudust` | ratatui 终端 UI | 聊天、管理 tool/skill/config、setup wizard |
| **服务器** | `garudust-server` | HTTP API | REST/SSE/WS + 多平台 bot、cron、指标 |

### 涉及的核心能力（都要保留）

- Agent 循环（think-tool-observe）
- 对话流式输出（SSE / Tauri event）
- 多会话管理
- 配置管理（config.yaml + .env）
- 工具管理（安装/卸载/更新）
- 技能管理
- Provider/Model 切换
- MCP 服务器连接
- Docker sandbox

---

## 改造方案：React + Tauri 桌面端

### 策略选择：**Option A — 嵌入式 Rust 后端（推荐）**

```
┌─────────────────────────────────────────────────────┐
│  Tauri Shell                                          │
│  ┌─────────────────┐  ┌───────────────────────────┐  │
│  │   React 前端      │  │   Rust 后端 (嵌入)        │  │
│  │                   │  │                           │  │
│  │  ChatView.tsx     │  │  garudust-agent (复用作   │  │
│  │  Sidebar.tsx      │◄─┤  为 Tauri commands)      │  │
│  │  SettngsPanel.tsx │  │  garudust-core            │  │
│  │  ToolMgr.tsx      │  │  garudust-tools           │  │
│  │  SkillMgr.tsx     │  │  garudust-transport       │  │
│  │                   │  │  garudust-memory          │  │
│  └─────────────────┘  │  garudust-cron             │  │
│                        │  garudust-gateway (可选)   │  │
│                        └───────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

**为什么选这个方案？**
- **所有 Rust crate 零改动复用**——不需要把 Rust 代码拆成 sidecar
- Tauri IPC 比 HTTP 更安全（不暴露网络端口）
- 延迟更低（进程内调用 vs HTTP 往返）
- 流式输出用 Tauri events 实现，天然支持

### 具体实现路径

#### 第一步：新项目骨架

```
garudust-desktop/                  # 新目录（或放在 workspace 里）
├── src-tauri/
│   ├── Cargo.toml                 # 依赖所有 garudust-* crate
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs                # Tauri 入口
│   │   └── cmd/                   # 按功能分模块
│   │       ├── chat.rs            # agent.run / agent.run_streaming
│   │       ├── config.rs          # 配置 CRUD
│   │       ├── tools.rs           # 工具管理
│   │       ├── skills.rs          # 技能管理
│   │       ├── sessions.rs        # 会话管理
│   │       └── system.rs          # 诊断/设置
│   └── icons/
├── src/                           # React 前端
│   ├── App.tsx
│   ├── components/
│   │   ├── ChatView.tsx           # 聊天界面（流式渲染，替代 TUI）
│   │   ├── Sidebar.tsx            # 侧边栏（会话列表+profile+skill）
│   │   ├── SettingsPanel.tsx      # 设置页面（替代 garudust config）
│   │   ├── ToolManager.tsx        # 工具管理（替代 garudust tool）
│   │   └── SkillManager.tsx       # 技能管理（替代 garudust skill）
│   └── hooks/
│       └── useAgentStream.ts      # Tauri event → React streaming
├── package.json
└── vite.config.ts
```

#### 第二步：Rust 端 Tauri Commands

**核心——把 `bin/garudust/src/main.rs` 里的逻辑搬到 Tauri commands**

```rust
// cmd/chat.rs
#[tauri::command]
async fn chat_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    message: String,
    session_key: Option<String>,
) -> Result<ChatResponse, String> {
    // 复用 Agent::run()，返回完整结果
}

#[tauri::command]
async fn chat_stream(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    message: String,
    session_key: Option<String>,
) -> Result<(), String> {
    // 复用 Agent::run_streaming()
    // 用 app.emit("chat:token", token) 推送到前端
}
```

**配置管理——替代 `garudust config show/set`**

```rust
#[tauri::command]
fn config_get(state: tauri::State<'_, AppState>) -> Result<AgentConfig, String> {
    Ok(state.config.as_ref().clone())
}

#[tauri::command]
fn config_set(state: tauri::State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    // 复用 config_cmd::set() 的逻辑
}
```

#### 第三步：React 前端

关键组件对应关系：

| 原来（Rust TUI） | 替代（React） |
|---|---|
| `ratatui` TUI 框架 | React + TailwindCSS / Ant Design |
| 终端聊天面板 | `<ChatView />` 流式消息气泡 |
| 侧边栏（profile/skill/toolset） | `<Sidebar />` 响应式侧边栏 |
| 键盘事件处理 | 表单输入 + Enter 提交 |
| `garudust config set` | `<SettingsPanel />` 表单 |
| `garudust tool install` | `<ToolManager />` 带搜索的 UI |
| `garudust skill list` | `<SkillManager />` 网格/列表 |

**流式聊天关键代码模式：**

```typescript
// hooks/useAgentStream.ts
import { listen } from '@tauri-apps/api/event';

export function useAgentStream() {
  const [tokens, setTokens] = useState<string[]>([]);
  
  useEffect(() => {
    const unlisten = listen<string>('chat:token', (event) => {
      setTokens(prev => [...prev, event.payload]);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);
  
  return tokens;
}
```

#### 第四步：保留的最大化代码

**可以原样复用的 crate（零改动）：**

| Crate | 行数 | 说明 |
|-------|------|------|
| `garudust-core` | ~2300+ | 核心 trait、类型、配置、错误定义 |
| `garudust-agent` | ~1600+ | Agent 循环（完全独立于 TUI） |
| `garudust-transport` | ~1500+ | LLM 通信层 |
| `garudust-tools` | ~5000+ | 工具注册表 + 全部工具集 |
| `garudust-memory` | ~1500+ | 记忆存储 |
| `garudust-cron` | ~200+ | 调度器 |

**需要改造的代码：**

| 文件 | 处理方式 |
|------|---------|
| `bin/garudust/src/main.rs` | 拆解为 Tauri commands（复用 `build_agent()`、`build_config()`） |
| `bin/garudust/src/tui.rs` | **完全替换**为 React 组件 |
| `bin/garudust/src/config_cmd.rs` | 改为 Tauri command（逻辑复用） |
| `bin/garudust/src/tool_cmd.rs` | 改为 Tauri command |
| `bin/garudust/src/skill_cmd.rs` | 改为 Tauri command |
| `bin/garudust/src/setup.rs` | 改为 React 引导页面 |
| `bin/garudust/src/doctor.rs` | 改为诊断页面 |

**可选保留的模块：**

| 模块 | 说明 |
|------|------|
| `garudust-gateway` | 可在桌面应用里内嵌一个"服务器模式"开关，启动 HTTP 服务 |
| `garudust-platforms` | 可选保留，让桌面应用也能跑 Telegram/Discord bot |

---

### 总结一下

**改造工作量评估：**

| 模块 | 工作量 | 说明 |
|------|--------|------|
| Rust 后端适配 | **小** | 把 main.rs/build_agent 逻辑搬到 Tauri commands |
| 核心 crate 复用 | **零** | 7 个 crate 全部原样依赖 |
| React 前端开发 | **中** | 聊天+设置+工具管理三个主要页面 |
| Tauri 集成 | **小** | 配置 tsconfig、vite、tauri.conf.json |
| **总计** | **2-3 周** | 一人全栈可完成 |

**核心思路就一句话：换个皮，不换心。** 把所有 crate 原封不动当 Tauri 的 Rust 依赖，`ratatui` 终端渲染换成 React + Tauri events，CLI 子命令换成对话框和表单——Agent 的脑子（`garudust-agent`）一根头发丝都不动。

要我现在开始搞吗？我可以先搭骨架——新建 `garudust-desktop/` 目录，写 Tauri + React 的初始代码。