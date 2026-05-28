# MCP 集成

jeeves 原生支持 Model Context Protocol (MCP)，可以连接任何MCP 服务器扩展能力。
## 什么是 MCP

Model Context Protocol (MCP) 是一个开放标准，允许 AI 模型与外部工具和服务进行通信。
## MCP 服务器管理
### 列出服务器
```bash
GET /api/mcp
```

### 创建服务器
```json
{
  "name": "服务器名称",
  "transport_type": "stdio",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"],
  "description": "文件系统 MCP 服务器"
}
```

### 连接状态
MCP 服务器支持以下状态：
- `connected`: 已连接- `disconnected`: 已断开
- `connecting`: 连接中- `error`: 连接错误

## 传输类型

jeeves 支持多种传输类型。
### 1. stdio

通过标准输入输出进行通信。
```json
{
  "transport_type": "stdio",
  "command": "python",
  "args": ["-m", "mcp_server"]
}
```

### 2. SSE

通过 Server-Sent Events 进行通信。
```json
{
  "transport_type": "sse",
  "url": "http://localhost:8000/mcp"
}
```

### 3. Streamable HTTP

通过流式 HTTP 进行通信。
```json
{
  "transport_type": "streamable",
  "url": "http://localhost:8000/mcp/stream"
}
```

## 内置 MCP 服务器
jeeves 内置了一些常用的 MCP 服务器：

### 文件系统服务器
提供文件系统操作能力。- 读取文件
- 写入文件
- 列出目录
- 删除文件

### 终端服务器
提供终端操作能力。- 执行命令
- 实时输出
- 交互式命令
## 工具发现

MCP 服务器连接后会自动发现可用工具：

1. 连接 MCP 服务器2. 发送工具发现请求3. 接收工具列表
4. 注册到工具系统。
## 自定义MCP 服务器
您可以创建自己的 MCP 服务器来扩展功能。
### 步骤

1. 安装 MCP SDK
2. 实现工具函数
3. 启动服务器4. 在 jeeves 中添加连接配置。
### 示例

```python
from mcp import MCP, Tool

mcp = MCP()

@mcp.tool
def hello_world(name: str) -> str:
    """Say hello to someone"""
    return f"Hello, {name}!"

if __name__ == "__main__":
    mcp.run()
```

## 安全考虑

### 权限控制

可以配置哪些工具允许被调用：

```json
{
  "allowed_tools": ["read_file", "list_dir"],
  "blocked_tools": ["execute_command", "delete_file"]
}
```

### 沙箱环境

建议在沙箱环境中运行不受信任的MCP 服务器。