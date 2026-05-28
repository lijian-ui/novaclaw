# API 接口文档

jeeves 后端提供完整的 RESTful API 接口，支持聊天、会话管理、文件操作、MCP 服务器管理、技能系统、定时任务、日志管理等功能。
## 基础信息

| 项目 | 说明 |
|------|------|
| **Base URL** | `http://localhost:3000/api` |
| **数据格式** | JSON |
| **认证方式** | 无（内部使用）|
| **错误响应** | `{ "success": false, "message": "错误信息" }` |
| **成功响应** | `{ "success": true, "data": {...} }` |

## 聊天相关 API

### 发送聊天消息（非流式）

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

### 发送聊天消息（流式 SSE）
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
- `type: chunk` - 文本块增加- `type: agent_step` - Agent 执行步骤
- `type: approval_required` - 需要用户确认- `type: done` - 完成
- `type: error` - 错误

### 工具执行确认

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

## 会话管理 API

### 列出所有会话
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

### 创建新会话
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

### 获取会话消息

```
GET /api/session?session_id=xxx&limit=50
```

### 删除会话

```
DELETE /api/session?session_id=xxx
```

## 文件操作 API

### 读取文件

```
POST /api/files/read
```

**请求体：**
```json
{
  "path": "/path/to/file.txt"
}
```

### 写入文件

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

### 列出目录

```
POST /api/files/list
```

**请求体：**
```json
{
  "path": "/path/to/directory"
}
```

### 删除文件/目录

```
POST /api/files/delete
```

**请求体：**
```json
{
  "path": "/path/to/delete"
}
```

## 模型配置 API

### 列出所有模型
```
GET /api/models
```

### 获取模型配置

```
GET /api/models-config
```

### 保存模型配置

```
PUT /api/models-config
```

## MCP 服务器 API

### 列出所有服务器

```
GET /api/mcp
```

### 创建服务器
```
POST /api/mcp
```

**请求体：**
```json
{
  "name": "服务器名称",
  "transport_type": "stdio",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem"]
}
```

### 删除服务器
```
DELETE /api/mcp/{name}
```

## 技能系统 API

### 列出所有技能
```
GET /api/skills
```

### 上传技能包

```
POST /api/skills/upload
```

**请求体：** multipart/form-data

### 删除技能
```
DELETE /api/skills/{id}
```

## 定时任务 API

### 列出所有任务
```
GET /api/cron-jobs
```

### 创建任务

```
POST /api/cron-jobs
```

**请求体：**
```json
{
  "name": "任务名称",
  "schedule": "0 * * * *",
  "payload": "任务消息内容"
}
```

### 更新任务

```
PUT /api/cron-jobs/{id}
```

### 删除任务

```
DELETE /api/cron-jobs/{id}
```

## 日志管理 API

### 获取系统日志

```
GET /api/logs?level=info
```

### 动态切换日志级别
```
POST /api/logs/level
```

**请求体：**
```json
{
  "level": "debug"
}
```

## 配置 API

### 获取项目配置

```
GET /api/config
```

### 更新项目配置

```
PUT /api/config
```