# 快速开始

欢迎使用 NovaClaw！本指南将帮助您在 2 分钟内完成安装并开始首次对话。

## 前置要求

- **Rust 1.70+**: 后端开发语言
- **Node.js 18+**: 前端开发环境
- **npm/yarn/pnpm**: 包管理器

## 安装步骤

### 1. 克隆项目

```bash
git clone https://gitee.com/rooky-top/novaclaw.git
cd novaclaw
```

### 2. 安装前端依赖

```bash
npm install
```

### 3. 启动后端服务

```bash
cd backend
cargo run
```

### 4. 启动前端开发服务器

在新终端中执行：

```bash
npm run dev
```

### 5. 打开应用

访问 `http://localhost:5173` 即可开始使用！


## 配置模型

首次使用前，请确保已配置好 LLM 模型：

1. 点击左侧菜单的 "设置" 图标
2. 进入 "模型配置" 页面
3. 添加您的 API Key 和模型配置
4. 选择默认模型

## 首次对话

1. 打开应用后，进入聊天界面
2. 在输入框中输入您的问题，例如："你好！"
3. 点击发送按钮，等待 Agent 响应



## 生产构建

```bash
# 前端构建
npm run build

# 后端构建
cd backend
cargo build --release

# Tauri 应用打包
npm run tauri:build
```

## 常见问题

### Q: 端口被占用怎么办？

修改 `config/config.json` 中的 `port` 字段来更换端口。

### Q: 如何使用本地模型？

在模型配置中添加本地模型的 Base URL，例如 Ollama 或 LM Studio。

### Q: 数据存储在哪里？

数据默认存储在：
- Windows: %USERPROFILE%\Documents\novaclaw\
- macOS: ~/Library/Application Support/novaclaw/
- Linux: ~/.local/share/novaclaw/