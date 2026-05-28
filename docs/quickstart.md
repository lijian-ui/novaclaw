# 快速开始

欢迎使用 Jeeves！本指南将帮助您在 2 分钟内完成安装并开始首次对话。

## 前置要求

| 依赖 | 版本 | 说明 |
|------|------|------|
| Rust | 1.70+ | 后端开发语言 |
| Node.js | 18+ | 前端开发环境 |
| npm/yarn/pnpm | 最新版 | 包管理器 |

### 安装 Rust（推荐使用 rustup）

```bash
# Windows（PowerShell）
iwr https://win.rustup.rs/x86_64 -OutFile rustup-init.exe; .\rustup-init.exe -y

# macOS/Linux
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

安装完成后重启终端或执行：
```bash
source "$HOME/.cargo/env"  # Linux/macOS
# Windows PowerShell: 重启终端即可
```

---

## 开发环境搭建

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

后端默认监听 `http://127.0.0.1:3000`，支持命令行参数：
```bash
cargo run -- --host 0.0.0.0 --port 8080
```

### 4. 启动前端开发服务器

在新终端中执行：

```bash
npm run dev
```

### 5. 打开应用

访问 `http://localhost:5173` 即可开始使用！

---

## 配置模型

首次使用前，请确保已配置 LLM 模型：

1. 点击左侧菜单 "设置" 图标
2. 进入 "模型配置" 页面
3. 添加您的 API Key 和模型配置
4. 选择默认模型

支持的模型提供商：
- OpenAI（GPT-3.5/4）
- DeepSeek
- LM Studio（本地）
- Ollama（本地）

---

## 首次对话

1. 打开应用后，进入聊天界面
2. 在输入框中输入您的问题，例如 "你好"
3. 点击发送按钮，等待 Agent 响应

---

## 生产构建

### Windows 打包客户端

**前置依赖**：
- Windows 10/11（64位）
- Visual Studio Build Tools（安装 "Desktop development with C++"）

**打包命令**：
```powershell
# 安装 Tauri CLI（首次）
npm install -g @tauri-apps/cli

# 前端构建
npm run build

# Tauri 应用打包
npm run tauri:build
```

**输出位置**：`src-tauri/target/release/bundle/`
- `.msi` - Windows 安装程序
- `.exe` - 独立可执行文件

### macOS 打包客户端

**前置依赖**：
- macOS 13+（Ventura）
- Xcode 14+（安装命令行工具）

```bash
# 安装 Xcode 命令行工具
xcode-select --install

# 安装 Homebrew（如未安装）
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 安装依赖
brew install rustup
rustup-init -y
source "$HOME/.cargo/env"
```

**打包命令**：
```bash
# 安装 Tauri CLI
npm install -g @tauri-apps/cli

# 前端构建
npm run build

# Tauri 应用打包
npm run tauri:build
```

**输出位置**：`src-tauri/target/release/bundle/dmg/`
- `.dmg` - macOS 磁盘镜像

### Linux 无界面后端打包

**前置依赖**（Ubuntu/Debian）：
```bash
sudo apt update && sudo apt install -y \
    build-essential \
    libssl-dev \
    pkg-config
```

**无界面后端打包**：
```bash
cd backend
cargo build --release --bin jeeves-server
```

**输出位置**：`backend/target/release/jeeves-server`

### Linux Tauri 桌面应用打包

**前置依赖**（Ubuntu/Debian）：
```bash
sudo apt update && sudo apt install -y \
    build-essential \
    libwebkit2gtk-4.0-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libssl-dev \
    pkg-config
```

**打包命令**：
```bash
npm run build
npm run tauri:build
```

**输出位置**：`src-tauri/target/release/bundle/`
- `.deb` - Debian/Ubuntu 安装包
- `.rpm` - Fedora/CentOS 安装包

---

## 部署指南

### 方式一：前后端分离部署（推荐）

**后端部署**：
```bash
# 启动后端（仅监听本地）
./jeeves-server --host 127.0.0.1 --port 3000
```

**前端部署**（使用 Nginx）：

创建 `/etc/nginx/sites-available/jeeves`：
```nginx
server {
    listen 80;
    server_name your-domain.com;

    # 前端静态文件
    location / {
        root /path/to/novaclaw/dist;
        try_files $uri $uri/ /index.html;
    }

    # API 代理
    location /api/ {
        proxy_pass http://127.0.0.1:3000/api/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }

    # WebSocket 代理
    location /ws/ {
        proxy_pass http://127.0.0.1:3000/ws/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

```bash
# 启用站点
sudo ln -s /etc/nginx/sites-available/jeeves /etc/nginx/sites-enabled/
sudo systemctl reload nginx
```

**配置 HTTPS**（使用 Let's Encrypt）：
```bash
sudo apt install certbot python3-certbot-nginx
sudo certbot --nginx -d your-domain.com
```

### 方式二：systemd 托管后端

创建 `/etc/systemd/system/jeeves.service`：
```ini
[Unit]
Description=Jeeves AI Agent Backend
After=network.target

[Service]
User=www-data
WorkingDirectory=/path/to/novaclaw/backend
ExecStart=/path/to/novaclaw/backend/target/release/jeeves-server --host 127.0.0.1 --port 3000
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable jeeves
sudo systemctl start jeeves

# 查看状态
sudo systemctl status jeeves
```

---

## 常见问题

### Q: 端口被占用怎么办？

修改启动参数：
```bash
./jeeves-server --port 8080
```

### Q: 如何使用本地模型？

在模型配置中添加本地模型的 Base URL：
- Ollama: `http://localhost:11434/v1`
- LM Studio: `http://localhost:1234/v1`

### Q: 数据存储在哪里？

数据默认存储在：
- Windows: `%USERPROFILE%\Documents\jeeves\`
- macOS: `~/Library/Application Support/jeeves/`
- Linux: `~/.local/share/jeeves/`

### Q: 生产环境安全建议

1. **禁止直接暴露后端端口**：使用 Nginx 反向代理，后端只绑定 `127.0.0.1`
2. **配置 HTTPS**：使用 Let's Encrypt 获取免费证书
3. **限制 CORS**：仅允许信任的域名访问 API
4. **定期更新依赖**：保持 Rust 和 Node.js 版本最新

### Q: 构建失败怎么办？

```bash
# 清理构建缓存
cargo clean

# 更新 Rust
rustup update

# 更新依赖
npm install
```

---

## 技术支持

如有问题，请提交 Issue 或联系开发团队。