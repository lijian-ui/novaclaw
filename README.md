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
