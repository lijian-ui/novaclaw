# 工具系统

NovaClaw 提供了丰富的内置工具集，支持文件操作、搜索分析、网络搜索、记忆管理等多种功能。

## 工具分类

### 文件操作工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `read_file` | 读取文件内容 | `path` | `offset`, `limit` |
| `write_file` | 写入文件（自动创建目录） | `path`, `content` | - |
| `edit_file` | 文件查找替换（单次替换） | `path`, `old_string`, `new_string` | - |
| `list_dir` | 列出目录内容 | - | `path`, `depth` |
| `rename_file` | 重命名或移动文件/目录 | `path`, `new_path` | - |
| `apply_patch` | 应用统一差异补丁 | `diff` | - |

### 搜索与分析工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `glob` | 按 glob 模式搜索文件 | `pattern` | `path` |
| `grep` | 在文件中搜索文本（正则表达式） | `pattern` | `path`, `include` |
| `search_replace` | 跨文件批量查找替换 | `pattern`, `replacement` | `path`, `include` |

### 网络搜索工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `web_search` | 网络搜索 | `query` | `count` |

#### 搜索引擎执行顺序

`web_search` 工具按以下顺序尝试搜索引擎，直至获取到结果：

| 优先级 | 搜索引擎 | API Key | 免费额度 | 说明 |
|-------|---------|---------|---------|------|
| 1 | **DuckDuckGo** | 不需要 | 无限制 | 免费优先使用，HTML 搜索 |
| 2 | **TinyFish** | [tinyfish_api_key](https://www.tinyfish.ai/) | 1000次/月 | TinyFish API，可选配置 |
| 3 | **Tavily** | [tavily_api_key](https://app.tavily.com/home) | 1000次/月 | Tavily API，可选配置 |

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
| `memory` | 持久化记忆管理 | `action` | `content`, `query`, `category` |
| `session_search` | 搜索历史会话消息 | `query` | `limit` |

### 技能、任务与计划工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `skill_view` | 查看技能完整内容 | `name` | - |
| `todo_write` | 写入完整待办列表（替换模式） | `items` | - |
| `todo_list` | 查看当前待办列表 | - | - |
| `submit_plan` | 提交执行计划给用户审批 | `goal`, `steps`, `summary` | - |

### 系统工具

| 工具名称 | 描述 | 必需参数 | 可选参数 |
|---------|------|---------|---------|
| `execute_command` | 执行 Shell 命令（PTY 伪终端） | `command` | `description`, `timeout`, `workdir` |

## 自定义工具

您可以通过 MCP 协议扩展自定义工具，详见 [MCP 集成文档](mcp.md)。