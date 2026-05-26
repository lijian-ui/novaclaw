# Jeeves

<p align="center">
  <b>现代化 AI Agent 桌面应用</b>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.70%2B-dea584?logo=rust" />
  <img src="https://img.shields.io/badge/React-18-61DAFB?logo=react" />
  <img src="https://img.shields.io/badge/Tauri-2.0-FFC131?logo=tauri" />
  <img src="https://img.shields.io/badge/Axum-0.7-000000?logo=rust" />
  <img src="https://img.shields.io/badge/MCP-Supported-blue" />
</p>

---

## ✨ 特性

- 🧠 **ReAct Agent** - 思维链可视化，推理过程透明可追溯
- 🔌 **MCP 原生支持** - 连接任意 MCP 服务器，无限扩展能力
- 🛠️ **20+ 内置工具** - 文件操作、终端仿真、网络搜索等
- 📦 **技能系统** - ZIP 格式技能包，易于导入导出分享
- ⏰ **定时任务** - 灵活 cron 表达式，支持后台自动执行
- 🎨 **现代 UI** - 可拖拽分栏、Markdown 渲染、暗色主题

---

## 🚀 快速开始

```bash
# 安装依赖
npm install

# 启动后端 (Rust)
cd backend && cargo run

# 启动前端 (新终端)
npm run dev

# 或使用 Tauri 开发模式
npm run tauri:dev
```

详细说明请查看 [快速开始](docs/quickstart.md)。

---

## 📚 文档

| 章节 | 内容 |
|------|------|
| [快速开始](docs/quickstart.md) | 安装 → 设置 → 2 分钟内开始首次对话 |
| [配置](docs/configuration.md) | 配置文件、提供商、模型、所有选项 |
| [工具系统](docs/tools.md) | 20+ 工具、工具调用、终端后端 |
| [技能系统](docs/skills.md) | 技能包、导入导出、创建技能 |
| [记忆](docs/memory.md) | 持久记忆、用户画像、最佳实践 |
| [MCP 集成](docs/mcp.md) | 连接任意 MCP 服务器扩展能力 |
| [定时调度](docs/cron.md) | 定时任务与平台投递 |
| [多智能体](docs/multi-agent.md) | Orchestrator + Worker 架构、子 Agent 管理、任务委派 |
| [架构](docs/architecture.md) | 项目结构、代理循环、关键模块 |
| [API 文档](docs/api.md) | 所有 API 端点参考 |
| [贡献](docs/contributing.md) | 开发设置、PR 流程、代码风格 |

---

## 📁 项目结构

```
jeeves/
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
  <b>Made with ❤️ by the jeeves Team</b>
</p>
