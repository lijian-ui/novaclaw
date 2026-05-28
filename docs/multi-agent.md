# 多智能体系统

jeeves 支持 **Profile + Orchestrator + Worker** 多智能体架构。

- **Profile**：每个智能体都有自己的身份（SOUL.md）、工具白名单、模型配置。用户可以在对话前选择一个Profile 作为 Leader。
- **Orchestrator**：Leader 智能体接收用户请求，遇到需要专业技能的任务时，自动委派给专门的子Agent。
- **Worker**：子 Agent 在独立上下文中执行任务，完成后返回结果给 Leader。

## 架构

```
用户 ──当Leader Agent（Orchestrator）
           │
           ├─ delegate_task("code-reviewer", "review 这段代码")
           │    ├─ 创建新Agent 会话（独立上下文）
           │    ├─ 子 Agent 使用专属系统提示词+ 工具集执行
           │    └─ 返回结果
           │
           ├─ delegate_task("data-analyst", "分析这些数据")
           │    └─ ...
           │
           └─ 整合所有子 Agent 结果 并回复用户
```

### 角色说明

| 角色 | 说明 |
|------|------|
| **Leader（Orchestrator）* | 主智能体，接收用户请求，决定何时委派任务 |
| **Worker（子 Agent。* | 专门的员工智能体，每个有独立的系统提示词和工具集 |

### 设计要点

- **LLM 自主决策**：LLM 通过 `delegate_task` 工具决定何时需要叫人，而非代码硬编码
- **上下文隔离*：子 Agent 的对话历史不写入主会话，不污当Leader 的上下文
- **独立 API 调用**：每个子 Agent 发起独立的LLM API 请求
- **限制迭代次数**：子 Agent 可配置`max_iterations`，防止失控
- **可自定义员工**：用户可在设置页面自由增删改员工

## 文件系统结构

每个智能体在 `agent/` 目录下拥有自己的文件夹：

```
jeeves/
└── agent/
    ├── default/
    │  └── SOUL.md                     # 主智能体的系统提示词
    ├── code-reviewer/
    │  ├── SOUL.md                     # 代码审查员提示词
    │  └── agent.json                  # 配置（工具白名单、模型等）
    ├── data-analyst/
    │  ├── SOUL.md
    │  └── agent.json
    └── web-researcher/
        ├── SOUL.md
        └── agent.json
```

### agent.json

```json
{
  "id": "code-reviewer",
  "name": "代码审查员",
  "description": "审查代码质量、发现Bug 和安全问题",
  "model": null,
  "enabled_tools": ["read_file", "search", "glob", "list_dir"],
  "max_iterations": 0
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 唯一标识，也是目录名 |
| `name` | string | 显示名称 |
| `description` | string | 简短描述|
| `model` | string\|null | 使用的模型，null 表示继承 Leader 的模型|
| `enabled_tools` | string[] | 允许的工具白名单，空数组 = 所有工具可用|
| `max_iterations` | number | 最大迭代次数，0 = 不限）|

### SOUL.md

`SOUL.md` 文件存放智能体的系统提示词，切换智能体时替换此文件内容作为系统提示词。支持安全扫描（Prompt Injection 检测）。

## 使用流程

### 1. 管理智能体

打开主控制台，点击 **智能体* 卡片 步骤：步骤：进入智能体管理页面。

- **查看智能体*：列表显示所有已注册智能体
- **添加智能体*：填写ID、名称、描述、系统提示词、模型、工具白名单
- **编辑智能体*：修改已有智能体配置
- **删除智能体*：删除整个智能体目录（默认智能体不可删除步骤：

### 2. 在对话中使用

聊天输入框右侧的 **🧠 智能体选择器* 可选择当前使用的Leader 智能体：

- **默认智能体*：使用系统默认提示词
- **自定义智能体**：使用对应的`SOUL.md` 作为系统提示词

### 3. 任务委派（自动）

当Leader 遇到需要专业技能的任务时，LLM 会自动调用`delegate_task` 工具。

```
用户说帮我 review 这段代码，顺便查一下北京的天气"
  步骤：
Leader 调用 delegate_task("code-reviewer", "review 代码...")
  步骤：创建独立会话 步骤：子 Agent 执行 步骤：返回结果
  步骤：
Leader 调用 delegate_task("web-researcher", "查询北京天气...")
  步骤：创建独立会话 步骤：子 Agent 执行 步骤：返回结果
  步骤：
Leader 整合结果回复用户
```

前端输入框上方会实时显示子Agent 的工作状态：
- 🔄 `代码审查员 review 用户注册功能的代码..` （运行中）
- 步骤：`代码审查员 任务完成` （完成，3 秒后消失）
- 步骤：`代码审查员 执行失败` （失败）

## 内置默认智能体

jeeves 首次启动时自动创建以下智能体：

| ID | 名称 | 描述 | 可用工具 |
|----|------|------|---------|
| `code-reviewer` | 代码审查员| 审查代码质量、Bug、安全|||| read_file, search, glob, list_dir |
| `data-analyst` | 数据分析师| 处理数据、统计分析、报告| read_file, write_file, execute_command, glob |
| `web-researcher` | 网络研究员| 搜索信息、整理资料| web_search, web_fetch, read_file, search |

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/agents` | 列出所有智能体 |
| PUT | `/api/agents/{id}` | 创建或更新智能体 |
| DELETE | `/api/agents/{id}` | 删除智能体|
| GET | `/api/set-agent/{id}` | 选择智能体（记录日志）|

## 常见问题

### 为什么LLM 不调用delegate_task？

LLM 有自己的判断逻辑。如果任务简单，LLM 可能选择自己处理而非委派。可以在系统提示词中加入委派规则来引导：

```
遇到以下任务时，你必须使用delegate_task 工具。
1. 代码审查 →委派给code-reviewer
2. 数据分析 →委派给data-analyst
3. 网络搜索 →委派给web-researcher
```

### 子 Agent 会消耗额外的API 费用吗？

会。每个子 Agent 发起独立的LLM API 请求。一次用户提问可能产生多次API 调用（Leader + 每个 Worker 各自的ReAct 循环）。

### 如何限制子 gent 的行为？

通过 `agent.json` 的`enabled_tools` 白名单限制可用工具。空数组表示所有工具可用）

### 子 Agent 会写入记忆吗？

不会。子 Agent 的临时对话不会影响Leader 的会话历史，也不会写入持久记忆。
# 多智能体系统

jeeves 支持 **Profile + Orchestrator + Worker** 多智能体架构。

- **Profile**：每个智能体都有自己的身份（SOUL.md）、工具白名单、模型配置。用户可以在对话前选择一个Profile 作为 Leader。
- **Orchestrator**：Leader 智能体接收用户请求，遇到需要专业技能的任务时，自动委派给专门的子Agent。
- **Worker**：子 Agent 在独立上下文中执行任务，完成后返回结果给 Leader。

## 架构

```
用户 ──当Leader Agent（Orchestrator）
           │
           ├─ delegate_task("code-reviewer", "review 这段代码")
           │    ├─ 创建新Agent 会话（独立上下文）
           │    ├─ 子 Agent 使用专属系统提示词+ 工具集执行
           │    └─ 返回结果
           │
           ├─ delegate_task("data-analyst", "分析这些数据")
           │    └─ ...
           │
           └─ 整合所有子 Agent 结果 并回复用户
```

### 角色说明

| 角色 | 说明 |
|------|------|
| **Leader（Orchestrator）* | 主智能体，接收用户请求，决定何时委派任务 |
| **Worker（子 Agent。* | 专门的员工智能体，每个有独立的系统提示词和工具集 |

### 设计要点

- **LLM 自主决策**：LLM 通过 `delegate_task` 工具决定何时需要叫人，而非代码硬编码
- **上下文隔离*：子 Agent 的对话历史不写入主会话，不污当Leader 的上下文
- **独立 API 调用**：每个子 Agent 发起独立的LLM API 请求
- **限制迭代次数**：子 Agent 可配置`max_iterations`，防止失控
- **可自定义员工**：用户可在设置页面自由增删改员工

## 文件系统结构

每个智能体在 `agent/` 目录下拥有自己的文件夹：

```
jeeves/
└── agent/
    ├── default/
    步骤：  └── SOUL.md                     # 主智能体的系统提示词
    ├── code-reviewer/
    步骤：  ├── SOUL.md                     # 代码审查员提示词
    步骤：  └── agent.json                  # 配置（工具白名单、模型等）
    ├── data-analyst/
    步骤：  ├── SOUL.md
    步骤：  └── agent.json
    └── web-researcher/
        ├── SOUL.md
        └── agent.json
```

### agent.json

```json
{
  "id": "code-reviewer",
  "name": "代码审查员",
  "description": "审查代码质量、发现Bug 和安全问题",
  "model": null,
  "enabled_tools": ["read_file", "search", "glob", "list_dir"],
  "max_iterations": 0
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 唯一标识，也是目录名 |
| `name` | string | 显示名称 |
| `description` | string | 简短描述|
| `model` | string\|null | 使用的模型，null 表示继承 Leader 的模型|
| `enabled_tools` | string[] | 允许的工具白名单，空数组 = 所有工具可用|
| `max_iterations` | number | 最大迭代次数，0 = 不限）|

### SOUL.md

`SOUL.md` 文件存放智能体的系统提示词，切换智能体时替换此文件内容作为系统提示词。支持安全扫描（Prompt Injection 检测）。

## 使用流程

### 1. 管理智能体

打开主控制台，点击 **智能体* 卡片 步骤：步骤：进入智能体管理页面。

- **查看智能体*：列表显示所有已注册智能体
- **添加智能体*：填写ID、名称、描述、系统提示词、模型、工具白名单
- **编辑智能体*：修改已有智能体配置
- **删除智能体*：删除整个智能体目录（默认智能体不可删除步骤：

### 2. 在对话中使用

聊天输入框右侧的 **🧠 智能体选择器* 可选择当前使用的Leader 智能体：

- **默认智能体*：使用系统默认提示词
- **自定义智能体**：使用对步骤：`SOUL.md` 作为系统提示词

### 3. 任务委派（自动）

当Leader 遇到需要专业技能的任务时，LLM 会自动调用`delegate_task` 工具。

```
用户说帮我 review 这段代码，顺便查一下北京的天气"
  步骤：
Leader 调用 delegate_task("code-reviewer", "review 代码...")
  步骤：创建独立会话 步骤：子 Agent 执行 步骤：返回结果
  步骤：
Leader 调用 delegate_task("web-researcher", "查询北京天气...")
  步骤：创建独立会话 步骤：子 Agent 执行 步骤：返回结果
  步骤：
Leader 整合结果回复用户
```

前端输入框上方会实时显示子Agent 的工作状态：
- 🔄 `代码审查员 review 用户注册功能的代码..` （运行中）
- 步骤：`代码审查员 任务完成` （完成，3 秒后消失）
- 步骤：`代码审查员 执行失败` （失败）

## 内置默认智能体

jeeves 首次启动时自动创建以下智能体：

| ID | 名称 | 描述 | 可用工具 |
|----|------|------|---------|
| `code-reviewer` | 代码审查员| 审查代码质量、Bug、安全|||| read_file, search, glob, list_dir |
| `data-analyst` | 数据分析师| 处理数据、统计分析、报告| read_file, write_file, execute_command, glob |
| `web-researcher` | 网络研究员| 搜索信息、整理资料| web_search, web_fetch, read_file, search |

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/agents` | 列出所有智能体 |
| PUT | `/api/agents/{id}` | 创建或更新智能体 |
| DELETE | `/api/agents/{id}` | 删除智能体|
| GET | `/api/set-agent/{id}` | 选择智能体（记录日志）|

## 常见问题

### 为什么LLM 不调用delegate_task？

LLM 有自己的判断逻辑。如果任务简单，LLM 可能选择自己处理而非委派。可以在系统提示词中加入委派规则来引导：

```
遇到以下任务时，你必须使用delegate_task 工具。
1. 代码审查 →委派给code-reviewer
2. 数据分析 →委派给data-analyst
3. 网络搜索 →委派给web-researcher
```

### 子 Agent 会消耗额外的API 费用吗？

会。每个子 Agent 发起独立的LLM API 请求。一次用户提问可能产生多次API 调用（Leader + 每个 Worker 各自的ReAct 循环）。

### 如何限制子 gent 的行为？

通过 `agent.json` 的`enabled_tools` 白名单限制可用工具。空数组表示所有工具可用）

### 子 Agent 会写入记忆吗？

不会。子 Agent 的临时对话不会影响Leader 的会话历史，也不会写入持久记忆。
