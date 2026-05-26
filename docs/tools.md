# 工具系统

Jeeves 提供了 21 个内置工具，覆盖文件操作、搜索分析、网络搜索、记忆管理、任务调度、IM 推送等多种功能。

## 工具分类

### 文件操作工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `read_file` | 读取文件内容 | `path` | `offset`, `limit` |
| `write_file` | 写入文件（自动创建目录） | `path`, `content` | - |
| `edit_file` | 文件查找替换（仅替换第一次出现） | `path`, `old_string`, `new_string` | - |
| `list_dir` | 列出目录内容（含名称、类型、大小） | - | `path`, `depth` |
| `rename_file` | 重命名或移动文件/目录 | `path`, `new_path` | - |
| `apply_patch` | 应用 unified diff 补丁到文件 | `diff` | - |

### 搜索与分析工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `glob` | 按 glob 模式搜索文件（如 `**/*.rs`） | `pattern` | `path` |
| `grep` | 在文件中搜索文本（正则表达式） | `pattern` | `path`, `include` |
| `search_replace` | 跨文件批量查找替换（正则） | `pattern`, `replacement` | `path`, `include` |

### 网络搜索工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `web_search` | 网络搜索（DuckDuckGo / TinyFish / Tavily） | `query` | `count` |

#### 搜索引擎执行顺序

`web_search` 工具按以下顺序尝试搜索引擎，直至获取到结果：

| 优先级 | 搜索引擎 | API Key | 免费额度 | 说明 |
|-------|---------|---------|---------|------|
| 1 | **DuckDuckGo** | 不需要 | 无限制 | 免费优先使用，HTML 搜索 |
| 2 | **TinyFish** | `tinyfish_api_key` | 1000次/月 | TinyFish API，可选配置 |
| 3 | **Tavily** | `tavily_api_key` | 1000次/月 | Tavily API，可选配置 |

#### API Key 配置

配置文件路径：`系统配置目录/config/config.json`

```json
{
  "tinyfish_api_key": "your-tinyfish-api-key",
  "tavily_api_key": "your-tavily-api-key"
}
```

**配置目录位置**：
- **Windows**: `%USERPROFILE%\Documents\novaclaw\config\config.json`
- **macOS**: `~/Library/Application Support/novaclaw/config/config.json`
- **Linux**: `~/.config/novaclaw/config.json`

> **提示**：DuckDuckGo 无需配置，免费使用。当前一个搜索引擎失败时，会自动尝试下一个。

### 记忆与会话工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `memory` | 跨会话持久化记忆管理。Actions: add/save/search/replace/remove/list | `action` | `content`, `query`, `category` |
| `session_search` | 搜索当前会话历史中的临时信息 | `query` | `limit` |

### 技能工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `skill_view` | 加载技能的完整指令和资源（含 linked_files 清单） | `name` | `file_path` |

### 任务与计划工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `todo_write` | 写入完整待办列表（替换模式）。每个任务有 status 和 priority | `items` | - |
| `todo_list` | 查看当前会话的待办任务列表 | - | - |
| `submit_plan` | 提交执行计划给用户审批（3步以上或有风险操作） | `goal`, `steps`, `summary` | - |

### 命令执行工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `execute_command` | 同步执行 Shell 命令，原地等待结果 | `command` | `description`, `timeout`, `workdir` |
| `execute_command_bg` | 后台执行 Shell 命令，立即返回 task_id（适用于长时间运行命令） | `command` | `description`, `workdir` |
| `poll_command` | 查询后台命令执行状态 | `task_id` | - |

### 系统管理工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `cron` | 管理定时任务。Actions: list/create/get/update/remove/run | `action` | `name`, `schedule`, `payload`, `id` |
| `delegate_task` | 委托子任务给专门的子 Agent（支持并行委托多个） | `agent_id`, `task` | - |

### IM 推送工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `im_push` | 向 IM 平台（钉钉等）通过指定机器人账号发送消息 | `robot`, `target_type`, `target_id`, `content` | `content_type`, `title` |

#### im_push 参数说明

| 参数 | 说明 |
|------|------|
| `robot` | 机器人账号 ID（如 `bot1`, `bot2`, `default`） |
| `target_type` | `private`（私聊）或 `group`（群聊） |
| `target_id` | 私聊：用户 userId；群聊：群 openConversationId |
| `content` | 消息内容（纯文本或 Markdown） |
| `content_type` | 消息格式，`text` 或 `markdown`（默认 text） |
| `title` | Markdown 标题（content_type=markdown 时必须） |

## 自定义工具

您可以通过 MCP 协议扩展自定义工具，详见 [MCP 集成文档](mcp.md)。

## 工具汇总统计

| 分类 | 数量 | 工具 |
|------|------|------|
| 文件操作 | 6 | read_file, write_file, edit_file, list_dir, rename_file, apply_patch |
| 搜索分析 | 3 | glob, grep, search_replace |
| 网络搜索 | 1 | web_search |
| 记忆会话 | 2 | memory, session_search |
| 技能 | 1 | skill_view |
| 任务计划 | 3 | todo_write, todo_list, submit_plan |
| 命令执行 | 3 | execute_command, execute_command_bg, poll_command |
| 系统管理 | 2 | cron, delegate_task |
| IM 推送 | 1 | im_push |
| **总计** | **21** | |
