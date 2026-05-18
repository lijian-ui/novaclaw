# NovaClaw

<p align="center">
  <b>现代化 AI Agent 桌面应用 - ReAct 架构、高性能、可扩展</b>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.70%2B-dea584?logo=rust" />
  <img src="https://img.shields.io/badge/React-18-61DAFB?logo=react" />
  <img src="https://img.shields.io/badge/Tauri-2.0-FFC131?logo=tauri" />
  <img src="https://img.shields.io/badge/Axum-0.7-000000?logo=rust" />
  <img src="https://img.shields.io/badge/MCP-Supported-blue?logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PHBhdGggZmlsbD0iI2ZmZiIgZD0iTTEyIDJDMS41IDIyIDMgMTIgMyAzLjUgMTIgMTAuNSAxMiAxNy41IDE3IDExIDEyIDMgOSAxNyAyMiAxMC41IDMgMjIgMiAyMiAxMiAyMi41IDExIDEyIDIuNSAxMiAyeiIvPjwvc3ZnPg==" />
</p>

---

## ✨ 核心优势

### 🚀 极致性能
- **Rust 后端**: 零成本抽象、内存安全、并发性能卓越
- **Axum 异步框架**: 非阻塞 I/O、高并发处理能力
- **Tauri 轻量级**: 比 Electron 快 10x，内存占用减少 50%+

### 🧠 真正的 ReAct Agent
- **思维链 (CoT)**: 透明的推理过程，可追溯 Agent 的思考
- **动态工具调用**: 基于上下文智能选择和执行工具
- **记忆系统**: 长期/短期记忆，支持上下文压缩
- **多轮对话**: 完整的会话管理，支持中断恢复

### 🔌 MCP 原生支持
- **Model Context Protocol**: 行业标准，无限扩展生态
- **多传输支持**: stdio、SSE、Streamable HTTP
- **热插拔工具**: 发现、连接、调用一体化管理

### 🛠️ 内置强大工具集
- **文件系统**: 读取、写入、编辑、搜索
- **终端仿真**: PTY 伪终端，支持实时交互
- **网络搜索**: TinyFish、Tavily 集成
- **技能系统**: ZIP 格式技能包，自定义工作流

### 🎨 现代 UI/UX
- **可拖拽分栏**: 灵活的布局配置
- **Markdown 渲染**: 支持 GFM、Mermaid 图表
- **代码高亮**: 语法高亮显示
- **暗色主题**: 护眼模式，轻松切换

---

## 🏗️ 架构设计

```
┌─────────────────────────────────────────────────────────────────┐
│                         NovaClaw Frontend                       │
│  ┌──────────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  Chat Panel      │  │  Dashboard   │  │  File Explorer   │  │
│  │  (React + TS)    │  │  (Grid)      │  │  (Editor + Tree) │  │
│  └──────────────────┘  └──────────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Axum HTTP/WebSocket                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │   Chat   │  │  Files   │  │  Tools   │  │   MCP    │      │
│  │  Routes  │  │  Routes  │  │  Routes  │  │  Routes  │      │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         NovaClaw Core                           │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌──────────┐ │
│  │   Agent    │  │   Tools    │  │   Memory   │  │   MCP    │ │
│  │  Runtime   │  │  Registry  │  │   Store    │  │  Bridge  │ │
│  └────────────┘  └────────────┘  └────────────┘  └──────────┘ │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐              │
│  │   LLM      │  │  Skills    │  │   Cron     │              │
│  │  Client    │  │  Loader    │  │  Scheduler │              │
│  └────────────┘  └────────────┘  └────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 📦 技术栈

### 后端
| 技术 | 用途 |
|------|------|
| **Rust 1.70+** | 核心开发语言 |
| **Tokio** | 异步运行时 |
| **Axum 0.7** | Web 框架 |
| **Tracing** | 结构化日志 |
| **RMCP** | MCP 客户端协议 |
| **Portable-pty** | 伪终端仿真 |

### 前端
| 技术 | 用途 |
|------|------|
| **React 18** | UI 框架 |
| **TypeScript** | 类型安全 |
| **Vite 5** | 构建工具 |
| **Tailwind CSS** | 样式系统 |
| **Radix UI** | 无样式组件库 |
| **Lucide** | 图标系统 |
| **React Router** | 路由管理 |

### 跨平台
| 技术 | 用途 |
|------|------|
| **Tauri 2.0** | 桌面应用框架 |
| **SSE/WebSocket** | 实时通信 |

---

## 🚀 快速开始

### 前置要求
- Rust 1.70+
- Node.js 18+
- npm/yarn/pnpm

### 开发模式

```bash
# 1. 安装前端依赖
npm install

# 2. 启动后端
cd backend
cargo run

# 3. 启动前端 (新终端)
npm run dev

# 或者使用 Tauri 一体化开发
npm run tauri:dev
```

### 生产构建

```bash
# 前端构建
npm run build

# 后端构建
cd backend
cargo build --release

# Tauri 应用打包
npm run tauri:build
```

---

## 📖 核心功能

### 🤖 Agent 系统
- **ReAct 模式**: 推理-行动循环
- **CoT 提取**: 思维链可视化
- **工具规划**: 智能任务分解
- **上下文压缩**: 自动历史消息压缩

### 🔧 工具系统
- **内置工具**: 20+ 内置工具开箱即用
- **自定义工具**: 灵活的注册接口
- **权限控制**: 可配置的安全策略

### 🔌 MCP 集成
- **服务发现**: 自动扫描 MCP 服务器
- **工具调用**: 透明的跨服务调用
- **连接管理**: 心跳检测、重连机制

### 📦 技能系统
- **技能包**: ZIP 格式，易于分发
- **导入导出**: 一键分享你的技能
- **版本管理**: 技能版本控制

### ⏰ Cron 任务
- **定时执行**: 灵活的 cron 表达式
- **任务管理**: 暂停、恢复、删除
- **日志记录**: 完整的执行历史

---

## 📂 数据目录结构

NovaClaw 会在用户目录下创建一个名为 `novaclaw` 的主数据目录，用于存储所有用户数据、配置和文件。

### 主数据目录位置

| 平台   | 路径示例                                  |
|--------|-------------------------------------------|
| **Windows** | `%USERPROFILE%\Documents\novaclaw\         |
| **macOS**   | `~/Library/Application Support/novaclaw/    |
| **Linux**   | `~/.local/share/novaclaw/                   |

### 目录结构树

```
novaclaw/
├── config/              # 配置文件目录
│   ├── config.json    # 项目配置（端口、CORS、API Key 等）
│   └── models.json    # 模型配置（LLM 提供商、API Key 等）
├── workspace/         # 工作目录（Agent 文件读写操作的基础目录）
├── skills/            # 技能目录（存放用户导入和自定义技能包）
├── memories/         # 记忆目录（持久化记忆存储）
├── sessions/        # 会话目录（聊天历史记录）
├── logs/            # 日志目录（运行日志）
└── cron/            # 定时任务目录（Cron 任务配置）
```

### 各目录详细说明

#### config/ - 配置目录
- **config.json**: 项目配置文件
  - HTTP 服务器端口、监听地址
  - CORS 允许来源
  - LLM 请求超时和重试配置
  - Agent 迭代次数、温度参数
  - 上下文压缩配置
  - 网络搜索 API Key（TinyFish / Tavily）
  - Prompt 注入保护开关

- **models.json**: 模型配置文件
  - 默认模型选择
  - LLM 提供商列表（多个）
  - 各提供商 API Key、Base URL
  - 支持的模型列表

> 💡 **提示**: 大部分配置项都可以通过前端界面直接修改，**无需手动编辑配置文件**。除非有特殊需求，否则建议使用前端进行配置管理。

#### workspace/ - 工作目录
- Agent 进行文件读写、编辑、搜索等操作的基础工作区
- 所有 `read_file`、`write_file` 等工具的默认根目录
- 可以通过配置 `data_dir` 自定义此路径

#### skills/ - 技能目录
- 存放用户导入和自定义的技能包（.zip 格式）
- 技能可以被 Agent 调用执行
- 支持导入、导出、分享

#### memories/ - 记忆目录
- 持久化记忆的长期记忆
- 按分类存储
- Agent 可以随时查询和引用

#### sessions/ - 会话目录
- 所有聊天会话记录
- 会话历史、消息记录
- 支持恢复之前的会话

#### logs/ - 日志目录
- 后端运行日志
- 调试和故障排查使用

#### cron/ - 定时任务目录
- Cron 任务配置存储
- 定时任务的执行记录

### 自定义数据目录

如果需要使用自定义路径，可以在 `config.json` 中设置 `data_dir` 字段：

```json
{
  "data_dir": "D:\\my-novaclaw-data"
}
```

### 配置文件优先级

1. **环境变量**: `NOVACLAW_CONFIG` 指定自定义配置文件路径（最高优先级）
2. **配置文件**: `config.json` / `models.json` 中的值
3. **默认值**: 代码中定义的默认值（最低优先级）

### 配置文件说明

#### config.json 完整示例

```json
{
  "port": 3000,
  "host": "0.0.0.0",
  "llm_timeout": 180,
  "max_retries": 3,
  "max_iterations": 0,
  "temperature": 0.7,
  "compact_threshold": 40,
  "compact_keep": 20,
  "allowed_origins": [
    "http://localhost:1420",
    "http://localhost:5173",
    "http://127.0.0.1:1420",
    "http://127.0.0.1:5173",
    "tauri://localhost"
  ],
  "prompt_injection_protection": true,
  "data_dir": null,
  "tinyfish_api_key": null,
  "tavily_api_key": null
}
```

#### models.json 完整示例

```json
{
  "default_model": "gpt-4o",
  "providers": [
    {
      "name": "openai",
      "api_key": "your-api-key",
      "base_url": "https://api.openai.com/v1",
      "models": ["gpt-4o", "gpt-4-turbo", "gpt-3.5-turbo"]
    }
  ]
}
```

---

## 🛠️ 内置工具列表

### 文件操作

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `read_file` | 读取文件内容 | `path` | `offset` (起始行), `limit` (最大行数) |
| `write_file` | 写入文件（自动创建目录） | `path`, `content` | - |
| `edit_file` | 文件查找替换（单次替换） | `path`, `old_string`, `new_string` | - |
| `list_dir` | 列出目录内容 | - | `path` (目录路径), `depth` (递归深度) |
| `delete_file` | 删除文件或目录（递归） | `path` | - |
| `rename_file` | 重命名或移动文件/目录 | `path`, `new_path` | - |
| `apply_patch` | 应用统一差异补丁 | `diff` | - |

### 搜索与分析

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `glob` | 按 glob 模式搜索文件 | `pattern` | `path` (根目录) |
| `grep` | 在文件中搜索文本（正则表达式） | `pattern` | `path` (目录), `include` (文件过滤器) |
| `search_replace` | 跨文件批量查找替换 | `pattern`, `replacement` | `path` (目录), `include` (文件过滤器) |
| `lsp` | 语义代码分析 | `action`, `file` | `symbol`, `line`, `character` |

### 网络搜索

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `web_search` | 网络搜索（DuckDuckGo / TinyFish / Tavily） | `query` | `count` (结果数量) |

### 记忆与会话

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `memory` | 持久化记忆管理 | `action` (add/query/remove) | `content` (内容), `query` (关键词), `category` (分类) |
| `session_search` | 搜索历史会话消息 | `query` | `limit` |

### 技能与任务

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `skill_view` | 查看技能完整内容 | `name` | - |
| `todo` | 简单任务管理 | `action` (add/list/done/remove) | `title` (标题), `id` (任务ID) |

### 系统工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `execute_command` | 执行 Shell 命令（PTY 伪终端） | `command` | `description`, `timeout`, `workdir` |

### LSP 语义分析支持

- **`definition`**: 查找定义位置
- **`references`**: 查找所有引用
- **`diagnostics`**: 获取编译/ lint 错误
- **`hover`**: 获取类型信息/文档

支持语言：Rust, TypeScript/JavaScript, Python, Go, Java

---

## 🎯 使用场景

1. **代码助手**: 智能代码生成、重构、审查
2. **任务自动化**: 文件处理、数据转换、报表生成
3. **知识管理**: 文档整理、信息提取、问答系统
4. **研究助手**: 文献检索、数据分析、报告撰写

---

## 📁 项目结构

```
novaclaw/
├── backend/
│   ├── src/
│   │   ├── agent/          # Agent 运行时、会话管理
│   │   ├── tools/          # 工具系统、内置工具
│   │   ├── mcp.rs          # MCP 协议集成
│   │   ├── skills/         # 技能加载器
│   │   ├── memory/         # 记忆系统
│   │   ├── llm/            # LLM 客户端
│   │   ├── cron/           # 定时任务
│   │   └── server/         # HTTP/WebSocket 服务器
│   └── Cargo.toml
├── src/
│   ├── components/         # React 组件
│   ├── pages/              # 页面组件
│   ├── hooks/              # 自定义 Hooks
│   ├── contexts/           # Context 提供者
│   └── i18n/               # 国际化
├── src-tauri/              # Tauri 配置
└── package.json
```

---

## 📡 API 接口文档

NovaClaw 后端提供完整的 RESTful API 接口，支持聊天、会话管理、文件操作、MCP 服务器管理、技能系统、定时任务、日志管理等功能。所有接口统一前缀为 `/api`。

### 基础信息

| 项目 | 说明 |
|------|------|
| **Base URL** | `http://localhost:3000/api` (本地开发) |
| **数据格式** | JSON |
| **认证方式** | 无（内部使用） |
| **错误响应** | `{ "success": false, "message": "错误信息" }` |
| **成功响应** | `{ "success": true, "data": {...} }` |

### 聊天相关 API

#### 1. 发送聊天消息（非流式）
```
POST /api/chat
```
**请求体：**
```json
{
  "session_id": "可选的会话ID",
  "message": "用户消息内容",
  "model": "可选的模型名称"
}
```
**响应：**
```json
{
  "success": true,
  "data": {
    "session_id": "会话ID",
    "content": "助手回复内容"
  }
}
```

#### 2. 发送聊天消息（流式 SSE）
```
POST /api/chat/stream
```
**请求体：**
```json
{
  "session_id": "可选的会话ID",
  "message": "用户消息内容",
  "model": "可选的模型名称",
  "workspace": "可选的工作目录路径"
}
```
**SSE 事件流：**
- `type: chunk` - 文本块增量
- `type: agent_step` - Agent 执行步骤（思考、工具调用等）
- `type: approval_required` - 需要用户确认
- `type: done` - 完成
- `type: error` - 错误

#### 3. 取消聊天流
```
POST /api/chat/cancel
```
**请求体：**
```json
{
  "session_id": "会话ID"
}
```

#### 4. 测试模型连接
```
POST /api/chat/test
```
**请求体：**
```json
{
  "api_key": "API密钥",
  "base_url": "https://api.openai.com/v1",
  "model": "gpt-4o"
}
```

#### 5. 工具执行确认（流式 SSE）
```
POST /api/chat/approve
```
**请求体：**
```json
{
  "approval_id": "确认ID",
  "session_id": "会话ID",
  "approved": true
}
```
**功能：** 用户确认后自动继续 Agent 执行，支持流式输出

---

### 会话管理 API

#### 1. 列出所有会话
```
GET /api/sessions
```
**响应：**
```json
{
  "success": true,
  "data": [
    {
      "id": "会话ID",
      "name": "会话名称",
      "model": "模型",
      "created_at": "创建时间",
      "updated_at": "更新时间"
    }
  ]
}
```

#### 2. 创建新会话
```
POST /api/sessions
```
**请求体：**
```json
{
  "name": "会话名称",
  "model": "可选的模型"
}
```

#### 3. 获取会话消息
```
GET /api/session?session_id=xxx&limit=50
```
**查询参数：**
- `session_id` (必需): 会话 ID
- `limit` (可选): 消息数量限制，默认 100

#### 4. 删除会话
```
DELETE /api/session?session_id=xxx
```

---

### 模型配置 API

#### 1. 列出所有模型
```
GET /api/models
```
**响应：**
```json
{
  "success": true,
  "data": [
    {
      "id": "openai/gpt-4o",
      "name": "gpt-4o",
      "provider": "openai",
      "context_window": 128000,
      "max_tokens": 4096
    }
  ]
}
```

#### 2. 获取指定模型
```
GET /api/models/{id}
```
**路径参数：** `id` 格式为 `provider/model`，如 `openai/gpt-4o`

#### 3. 获取模型配置
```
GET /api/models-config
```

#### 4. 保存模型配置
```
PUT /api/models-config
```
**请求体：** 完整的 models.json 配置对象

#### 5. 设置默认模型
```
PUT /api/default-model
```
**请求体：**
```json
{
  "model": "gpt-4o"
}
```

---

### 项目配置 API

#### 1. 获取项目配置
```
GET /api/config
```
**响应：** config.json 的完整内容

#### 2. 更新项目配置
```
PUT /api/config
```
**请求体：** 完整的 config.json 配置对象

---

### 文件操作 API

#### 1. 读取文件
```
POST /api/files/read
```
**请求体：**
```json
{
  "path": "/path/to/file.txt"
}
```

#### 2. 写入文件
```
POST /api/files/write
```
**请求体：**
```json
{
  "path": "/path/to/file.txt",
  "content": "文件内容"
}
```

#### 3. 列出目录
```
POST /api/files/list
```
**请求体：**
```json
{
  "path": "/path/to/directory"
}
```

#### 4. 复制文件/目录
```
POST /api/files/copy
```
**请求体：**
```json
{
  "source": "/source/path",
  "dest": "/destination/path"
}
```

#### 5. 重命名/移动
```
POST /api/files/rename
```
**请求体：**
```json
{
  "old_path": "/old/path",
  "new_path": "/new/path"
}
```

#### 6. 删除文件/目录
```
POST /api/files/delete
```
**请求体：**
```json
{
  "path": "/path/to/delete"
}
```

#### 7. 创建目录
```
POST /api/files/mkdir
```
**请求体：**
```json
{
  "path": "/path/to/create"
}
```

#### 8. 获取布局配置
```
GET /api/files/layout
```

#### 9. 保存布局配置
```
POST /api/files/layout
```

#### 10. 清空缓存
```
POST /api/files/cache
```
**功能：** 删除 sessions 和 memories 目录内容

#### 11. 获取所有目录路径
```
GET /api/files/paths
```
**响应：**
```json
{
  "success": true,
  "data": {
    "config_dir": "配置目录路径",
    "data_dir": "数据目录路径",
    "workspace_dir": "工作目录路径",
    "sessions_dir": "会话目录路径",
    "memories_dir": "记忆目录路径",
    "skills_dir": "技能目录路径",
    "logs_dir": "日志目录路径"
  }
}
```

---

### MCP 服务器管理 API

#### 1. 列出所有 MCP 服务器
```
GET /api/mcp
```
**响应：**
```json
{
  "servers": [
    {
      "name": "服务器名称",
      "transport_type": "stdio",
      "status": "connected",
      "tools": ["tool1", "tool2"]
    }
  ]
}
```

#### 2. 创建 MCP 服务器
```
POST /api/mcp
```
**请求体：**
```json
{
  "name": "服务器名称",
  "transport_type": "stdio",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"],
  "description": "可选描述"
}
```

#### 3. 删除 MCP 服务器
```
DELETE /api/mcp/{name}
```

#### 4. 切换启用状态
```
POST /api/mcp/{name}/toggle
```

#### 5. 发现工具
```
POST /api/mcp/{name}/discover
```

#### 6. 手动连接
```
POST /api/mcp/{name}/connect
```

#### 7. 断开连接
```
POST /api/mcp/{name}/disconnect
```

---

### 技能系统 API

#### 1. 列出所有技能
```
GET /api/skills
```
**响应：**
```json
{
  "success": true,
  "data": [
    {
      "id": "skill-name",
      "name": "技能名称",
      "description": "技能描述",
      "version": "1.0.0",
      "enabled": true,
      "content": "技能内容"
    }
  ]
}
```

#### 2. 获取指定技能
```
GET /api/skills/{id}
```

#### 3. 上传技能包
```
POST /api/skills/upload
```
**请求体：** multipart/form-data，字段 `file` 为 .zip 格式的技能包

#### 4. 删除技能
```
DELETE /api/skills/{id}
```

---

### 定时任务 API

#### 1. 列出所有定时任务
```
GET /api/cron-jobs
```
**响应：**
```json
{
  "success": true,
  "data": [
    {
      "id": "任务ID",
      "name": "任务名称",
      "schedule": "0 * * * *",
      "enabled": true,
      "next_run_at": "下次执行时间",
      "last_run_at": "上次执行时间",
      "run_count": 5
    }
  ]
}
```

#### 2. 创建定时任务
```
POST /api/cron-jobs
```
**请求体：**
```json
{
  "name": "任务名称",
  "schedule": "0 * * * *",
  "payload": "任务消息内容",
  "session_id": "可选关联的会话ID"
}
```

#### 3. 获取指定任务
```
GET /api/cron-jobs/{id}
```

#### 4. 更新定时任务
```
PUT /api/cron-jobs/{id}
```
**请求体：**
```json
{
  "name": "新名称",
  "schedule": "0 0 * * *",
  "enabled": false
}
```

#### 5. 删除定时任务
```
DELETE /api/cron-jobs/{id}
```

#### 6. 切换启用状态
```
POST /api/cron-jobs/{id}/toggle
```

#### 7. 立即执行任务
```
POST /api/cron-jobs/{id}/run
```

---

### 日志管理 API

#### 1. 获取系统日志
```
GET /api/logs?level=info
```
**查询参数：** `level` (可选)，如 `trace`, `debug`, `info`, `warn`, `error`

**响应：**
```json
{
  "success": true,
  "data": [
    {
      "timestamp": "2024-01-01T12:00:00Z",
      "level": "info",
      "message": "日志内容",
      "target": "模块路径"
    }
  ]
}
```

#### 2. 动态切换日志级别
```
POST /api/logs/level
```
**请求体：**
```json
{
  "level": "debug"
}
```

#### 3. 列出任务日志
```
GET /api/logs/tasks
```
**响应：** 所有有日志的任务 ID 列表

#### 4. 获取任务日志
```
GET /api/logs/tasks/{task_id}
```

#### 5. 删除任务日志
```
DELETE /api/logs/tasks/{task_id}
```

---

### IM 渠道配置 API

#### 1. 获取 IM 渠道配置
```
GET /api/config/im_channels
```
**响应：**
```json
{
  "success": true,
  "channels": [
    {
      "id": "dingtalk",
      "enabled": true,
      "webhook": "https://oapi.dingtalk.com/robot/send?access_token=xxx",
      "secret": "可选的加密密钥"
    }
  ]
}
```

#### 2. 保存 IM 渠道配置
```
POST /api/config/im_channels
```
**请求体：**
```json
{
  "channels": [
    {
      "id": "dingtalk",
      "enabled": true,
      "webhook": "https://..."
    }
  ]
}
```

#### 3. 获取支持的渠道类型
```
GET /api/config/im_channel_types
```
**响应：**
```json
{
  "success": true,
  "types": [
    {
      "id": "dingtalk",
      "name": "钉钉",
      "fields": ["webhook", "secret"]
    },
    {
      "id": "feishu",
      "name": "飞书",
      "fields": ["webhook", "secret", "app_id"]
    },
    {
      "id": "wecom",
      "name": "企业微信",
      "fields": ["webhook"]
    }
  ]
}
```

---

## 🤝 贡献指南

欢迎提交 Issue 和 PR！项目使用标准 GitHub 工作流。

---

## 📄 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件

---

## 🙏 致谢

- [Anthropic MCP](https://modelcontextprotocol.io/) - 协议标准
- [Axum](https://github.com/tokio-rs/axum) - Web 框架
- [Tauri](https://tauri.app/) - 桌面应用框架

---

<p align="center">
  <b>Made with ❤️ by the NovaClaw Team</b>
</p>
