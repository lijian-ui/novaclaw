# 贡献指南

欢迎为jeeves 项目做出贡献！本指南将帮助您了解如何参与项目开发。
## 开发设置
### 前置要求

- Rust 1.70+
- Node.js 18+
- npm/yarn/pnpm
- Git

### 克隆项目

```bash
git clone https://gitee.com/rooky-top/jeeves.git
cd jeeves
```

### 安装依赖

```bash
# 前端依赖
npm install

# 后端依赖（自动安装）
cd backend
cargo build
```

### 运行开发服务器

```bash
# 启动后端（终端）：cd backend
cargo run

# 启动前端（终端）：npm run dev
```

## PR 流程

### 1. 创建分支

```bash
git checkout -b feature/your-feature-name
```

### 2. 提交代码

```bash
git add .
git commit -m "feat: 添加新功能
```

### 3. 推送到远程

```bash
git push origin feature/your-feature-name
```

### 4. 创建 Pull Request

在GitHub 上创建PR，描述您的改动。
## 代码风格

### Rust 代码风格

- 使用 `cargo fmt` 格式化代码- 使用 `cargo clippy` 检查代码- 遵循 Rust 官方代码风格指南

### TypeScript 代码风格

- 使用 `npm run lint` 检查代码- 使用 `npm run format` 格式化代码- 遵循 Airbnb 代码风格指南

### 提交信息规范

```
<type>: <description>

<optional body>
```

**类型说明：*
- `feat`: 新功能- `fix`: 修复 bug
- `docs`: 文档更新
- `style`: 代码风格
- `refactor`: 重构
- `test`: 测试
- `chore`: 构建/工具

## 测试

### 运行测试

```bash
# 后端测试
cd backend
cargo test

# 前端测试
npm test
```

### 编写测试

- 为新功能编写单元测试
- 确保测试覆盖率达到80%+
- 运行测试确保没有失败

## 文档

### 更新文档

- 修改代码后同步更新相关文档- 保持文档与代码一致- 添加必要的注释。
### 文档格式

- 使用 Markdown 格式
- 保持清晰的结构- 使用适当的标题层级
## 问题报告

### Bug 报告

1. 搜索现有 issue
2. 使用清晰的标题3. 提供复现步骤
4. 包含环境信息

### 功能请求

1. 描述功能需求2. 说明使用场景
3. 提供实现建议

## 许可证
所有贡献都将遵循MIT 许可证。