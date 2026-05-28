# 配置指南

jeeves 的配置系统分为项目配置和模型配置两部分，支持通过前端界面或手动编辑配置文件进行修改。
## 配置文件位置

配置文件默认存储在用户数据目录下的`config/` 文件夹中。
| 平台 | 路径 |
|------|------|
| Windows | `%USERPROFILE%\Documents\jeeves\config\` |
| macOS | `~/Library/Application Support/jeeves/config/` |
| Linux | `~/.local/share/jeeves/config/` |

## 项目配置 (config.json)

### 配置项说明
| 配置项| 类型 | 默认值| 说明 |
|--------|------|--------|------|
| `port` | number | 3000 | HTTP 服务器端口|
| `host` | string | "0.0.0.0" | 监听地址 |
| `llm_timeout` | number | 180 | LLM 请求超时时间（秒）|
| `max_retries` | number | 3 | 最大重试次数|
| `max_iterations` | number | 0 | Agent 最大迭代次数（0 表示无限制） |
| `temperature` | number | 0.7 | 温度参数 |
| `compact_threshold` | number | 40 | 上下文压缩阈值|
| `compact_keep` | number | 20 | 压缩后保留消息数 |
| `allowed_origins` | array | ["http://localhost:1420", "http://localhost:5173"] | CORS 允许来源 |
| `prompt_injection_protection` | boolean | true | Prompt 注入保护开关|
| `data_dir` | string | null | 自定义数据目录路径|
| `tinyfish_api_key` | string | null | TinyFish API Key |
| `tavily_api_key` | string | null | Tavily API Key |

### 完整示例

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

## 模型配置 (models.json)

### 配置项说明
| 配置项| 类型 | 说明 |
|--------|------|------|
| `default_model` | string | 默认模型名称 |
| `providers` | array | LLM 提供商列表|

### 提供商配置
每个提供商包含以下字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 提供商名称|
| `api_key` | string | API Key |
| `base_url` | string | API 基础 URL |
| `models` | array | 可用模型列表 |

### 完整示例

```json
{
  "default_model": "gpt-4o",
  "providers": [
    {
      "name": "openai",
      "api_key": "your-api-key",
      "base_url": "https://api.openai.com/v1",
      "models": ["gpt-4o", "gpt-4-turbo", "gpt-3.5-turbo"]
    },
    {
      "name": "deepseek",
      "api_key": "your-api-key",
      "base_url": "https://api.deepseek.com/v1",
      "models": ["deepseek-chat"]
    }
  ]
}
```

## 配置优先级
1. **环境变量**: `jeeves_CONFIG` 指定自定义配置文件路径（最高优先级）2. **配置文件**: `config.json` / `models.json` 中的值3. **默认值： 代码中定义的默认值（最低优先级）
## 通过前端配置

大部分配置项都可以通过前端界面直接修改，无需手动编辑配置文件。
1. 点击左侧菜单的"设置" 图标
2. 根据需要选择 "项目配置" 或"模型配置"
3. 修改配置项后点击保存

## 自定义数据目录
如果需要使用自定义路径，可以在 `config.json` 中设置`data_dir` 字段。
```json
{
  "data_dir": "D:\\my-jeeves-data"
}
```