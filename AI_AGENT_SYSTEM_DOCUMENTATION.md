# NovaClaw AI Agent 后端系统技术文档

## 概述

NovaClaw 是一个企业级、生产级的 AI Agent 后端系统，采用 Rust 语言实现，支持双部署架构：
- **Tauri 桌面客户端**：适用于 Windows/macOS
- **Axum Web 服务**：适用于 Linux 服务器

系统实现了完整的 AI Agent 功能体系，包括 ReAct 框架、Agent Loop、CoT 推理、提示词工程、工具系统、记忆系统、会话管理和技能系统。

---

## 1. ReAct（Reasoning and Acting）框架实现

### 1.1 整体架构设计

ReAct 框架是 NovaClaw 的核心推理引擎，遵循 **"思考-行动-观察"** 循环模式：

```
┌─────────────────────────────────────────────────────────────────┐
│                       ReAct 框架架构                            │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │   Thought   │────▶│   Action    │────▶│  Observation│       │
│  │  (LLM推理)  │     │  (工具调用)  │     │  (结果反馈)  │       │
│  └─────────────┘     └─────────────┘     └──────┬──────┘       │
│         ^                                        │              │
│         └────────────────────────────────────────┘              │
│                        循环迭代                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 核心组件构成

| 组件 | 职责 | 对应模块 |
|------|------|----------|
| **AgentRuntime** | 执行 ReAct 循环的核心运行时 | `agent/runtime.rs` |
| **AgentSession** | 管理对话状态和历史消息 | `agent/session.rs` |
| **SystemPromptBuilder** | 构建系统提示词 | `agent/prompt.rs` |
| **CotExtractor** | 提取思维链推理内容 | `agent/cot.rs` |
| **ToolRegistry** | 工具注册与执行 | `tools/registry.rs` |
| **LlmClient** | LLM API 客户端 | `llm/client.rs` |

### 1.3 推理与行动的交互流程

核心流程在 `agent/runtime.rs:60-238` 的 `run_turn` 方法中实现：

```rust
// 1. 记录用户输入
self.session.push_user(user_input);

// 2. 构建系统提示词
let system_prompt = self.build_system_prompt();

// 3. ReAct 主循环
loop {
    // 3.1 Thought: 调用 LLM 获取推理和行动
    let (assistant_message, reasoning) = self.call_llm_with_tools(&system_prompt, &step_tx).await?;
    
    // 3.2 提取工具调用
    let tool_calls: Vec<AgentToolCall> = assistant_message.tool_calls.clone().unwrap_or_default();
    
    // 3.3 检查终止条件：无工具调用则任务完成
    if tool_calls.is_empty() {
        final_content = assistant_message.content.clone();
        break;
    }
    
    // 3.4 Action: 执行工具调用
    for tc in &tool_calls {
        let tool_result = self.tool_registry.execute(&tc.name, args).await;
        self.session.push_tool_result(&tc.id, &tc.name, &tool_result);
    }
    // 继续循环...
}
```

### 1.4 关键设计模式

- **状态模式**：`AgentSession` 维护完整的对话状态
- **策略模式**：`ToolRegistry` 支持动态注册不同工具
- **观察者模式**：通过 `mpsc::Sender` 发送步骤事件到前端

---

## 2. Agent Loop 循环机制

### 2.1 循环实现方式

Agent Loop 在 `agent/runtime.rs:75-209` 实现，采用 **while 循环 + 条件退出** 模式：

```rust
loop {
    iterations += 1;
    
    // 防止死循环：超过最大迭代次数则终止
    if iterations > self.max_iterations {
        return Err(AppError::AgentError(format!(
            "超过最大迭代次数限制 ({})",
            self.max_iterations
        )));
    }
    
    // 执行单次推理-行动循环
    let (assistant_message, reasoning) = self.call_llm_with_tools(&system_prompt, &step_tx).await?;
    
    // 检查是否需要继续循环（是否有工具调用）
    let tool_calls = assistant_message.tool_calls.clone().unwrap_or_default();
    if tool_calls.is_empty() {
        final_content = assistant_message.content.clone();
        break;
    }
    
    // 执行工具并收集结果后继续循环
}
```

### 2.2 触发条件

| 触发时机 | 说明 |
|----------|------|
| 用户输入新消息 | 启动完整的 ReAct 循环 |
| 工具执行完成 | 触发下一轮推理 |
| 达到最大迭代次数 | 强制终止并返回错误 |

### 2.3 终止条件

```rust
// 终止条件1：无工具调用 → 任务完成
if tool_calls.is_empty() {
    final_content = assistant_message.content.clone();
    break;
}

// 终止条件2：超过最大迭代次数 → 错误终止
if iterations > self.max_iterations {
    return Err(AppError::AgentError(...));
}
```

### 2.4 状态管理

`AgentSession` 在 `agent/session.rs` 中维护完整状态：

```rust
pub struct AgentSession {
    pub id: String,                    // 会话唯一标识
    pub name: String,                  // 会话名称
    pub model: String,                 // 使用的模型
    pub messages: Vec<AgentMessage>,   // 消息历史
    pub workspace: Option<String>,     // 工作目录
    pub compaction_count: u32,         // 上下文压缩次数
    pub total_input_tokens: u64,       // 输入 Token 计数
    pub total_output_tokens: u64,      // 输出 Token 计数
}
```

### 2.5 异常处理机制

```rust
// LLM 调用错误处理
let result = runtime.run_turn(&user_msg, step_tx).await;
match result {
    Ok(agent_result) => { /* 成功处理 */ },
    Err(e) => {
        // 发送错误消息到前端
        sender.send(Message::Text(serde_json::json!({
            "type": "error",
            "data": {"message": e.to_string()}
        }).to_string())).await;
    }
}
```

---

## 3. 思维链（Chain of Thought, CoT）推理能力

### 3.1 CoT 提取实现

CoT 提取器在 `agent/cot.rs` 中实现，支持多提供商格式的统一抽象：

```rust
pub struct CotExtractor;

impl CotExtractor {
    /// 从助手回复中提取推理内容
    /// 支持多提供商格式：
    /// 1. reasoning_content 字段 (DeepSeek/OpenRouter)
    /// 2. reasoning 字段 (Qwen)  
    /// 3. 内联 thinking 标签 (fallback)
    pub fn extract(content: &str, reasoning_field: Option<&str>) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        
        // Level 1: reasoning_content 字段
        if let Some(r) = reasoning_field {
            if !r.is_empty() {
                parts.push(r.to_string());
            }
        }
        
        // Level 3: 内联 thinking 标签（兜底）
        if parts.is_empty() {
            if let Some(thinking) = Self::extract_inline_thinking(content) {
                parts.push(thinking);
            }
        }
        
        if parts.is_empty() { None } else { Some(parts.join("\n")) }
    }
}
```

### 3.2 推理步骤管理

在 `agent/runtime.rs:94-100` 累积推理内容：

```rust
// 累积推理内容
if let Some(ref r) = reasoning {
    if !r.is_empty() {
        if !self.accumulated_reasoning.is_empty() {
            self.accumulated_reasoning.push('\n');
        }
        self.accumulated_reasoning.push_str(r);
    }
}
```

### 3.3 上下文处理机制

CoT 内容通过以下方式融入对话上下文：

1. **流式接收**：从 LLM 流式响应中提取 `reasoning_content` 字段
2. **增量累积**：在循环中逐步累积推理内容
3. **结果封装**：最终返回包含完整推理链的 `AgentResult`

---

## 4. 提示词工程系统

### 4.1 动态提示词生成

`SystemPromptBuilder` 在 `agent/prompt.rs` 中实现，采用 **8 层组装模式**：

```rust
pub fn build(&self) -> String {
    let mut sections: Vec<String> = Vec::new();
    
    // 1. 身份定义层
    sections.push(self.build_identity());
    
    // 2. 系统规则层
    sections.push(self.build_system_rules());
    
    // 3. 任务执行层
    sections.push(self.build_task_execution());
    
    // 4. 静态/动态边界
    sections.push("---".to_string());
    
    // 5. 环境信息层
    sections.push(self.build_environment());
    
    // 6. 工具使用指导层
    sections.push(self.build_tool_guidance());
    
    // 7. 记忆使用指导层
    sections.push(self.build_memory_guidance());
    
    // 8. 技能索引层（可选）
    if !self.skill_list.is_empty() {
        sections.push(self.build_skill_index());
    }
    
    sections.join("\n\n")
}
```

### 4.2 提示词模板设计

**身份定义层**（`agent/prompt.rs:67-78`）：
```rust
fn build_identity(&self) -> String {
    r#"# 身份定义

你是 NovaClaw，一个企业级 AI Agent 助手。你能够帮助用户完成各种任务...
"#.to_string()
}
```

**环境信息层**（动态生成）：
```rust
fn build_environment(&self) -> String {
    format!(
        "# 环境信息\n\n- 操作系统: {}\n- 当前日期: {}\n- 工作目录: {}",
        self.os_name,
        chrono::Local::now().format("%Y-%m-%d"),
        self.workspace.unwrap_or_default()
    )
}
```

### 4.3 变量替换机制

支持的动态变量：
| 变量 | 来源 | 用途 |
|------|------|------|
| `os_name` | 编译时 `cfg!` 宏 | 标识操作系统 |
| `workspace` | 会话配置 | 当前工作目录 |
| `skill_list` | 技能加载器 | 可用技能列表 |

### 4.4 提示词优化策略

- **分层结构**：清晰的章节划分便于 LLM 理解
- **平台适配**：根据操作系统动态调整提示词
- **上下文感知**：工作目录和技能列表实时更新

---

## 5. 内置工具系统

### 5.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                      工具系统架构                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │  ToolDef    │───▶│ToolRegistry │───▶│  Handler    │     │
│  │ (工具定义)   │    │ (注册中心)   │    │ (执行函数)  │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│         │                  │                               │
│         ▼                  ▼                               │
│  ┌─────────────┐    ┌─────────────┐                        │
│  │  Schema     │    │  execute()  │                        │
│  │ (OpenAI格式) │    │ (执行入口)  │                        │
│  └─────────────┘    └─────────────┘                        │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 工具注册机制

在 `tools/registry.rs` 中实现线程安全的工具注册：

```rust
pub struct ToolRegistry {
    pub(crate) tools: Arc<RwLock<HashMap<String, ToolDef>>>,
}

impl ToolRegistry {
    /// 注册工具
    pub async fn register(&self, tool: ToolDef) {
        let mut tools = self.tools.write().await;
        tools.insert(tool.name.clone(), tool);
    }
    
    /// 获取工具定义
    pub async fn get(&self, name: &str) -> Option<ToolDef> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }
}
```

### 5.3 工具调用接口

工具定义结构：

```rust
pub struct ToolDef {
    pub name: String,                                   // 工具名称
    pub description: String,                            // 工具描述
    pub parameters: Value,                              // OpenAI Function Calling Schema
    pub handler: Arc<dyn Fn(Value) -> Result<String, String> + Send + Sync>,
}
```

### 5.4 内置工具实现

| 工具名称 | 功能 | 实现位置 |
|----------|------|----------|
| `read_file` | 读取文件内容 | `tools/builtin.rs:16-48` |
| `write_file` | 写入文件 | `tools/builtin.rs:50-81` |
| `edit_file` | 编辑文件（查找替换） | `tools/builtin.rs:83-124` |
| `glob` | 按模式搜索文件 | `tools/builtin.rs:126-173` |
| `grep` | 文本内容搜索 | `tools/builtin.rs:175-250` |
| `memory` | 持久化记忆管理 | `tools/builtin.rs:252-308` |
| `session_search` | 搜索历史会话 | `tools/builtin.rs:310-335` |
| `web_search` | 网络搜索 | `tools/builtin.rs:337-355` |
| `todo` | 任务管理 | `tools/builtin.rs:357-393` |

**`read_file` 工具示例**（`tools/builtin.rs:37-47`）：

```rust
handler: Arc::new(|args: serde_json::Value| -> Result<String, String> {
    let path = args["path"].as_str().ok_or("缺少 path 参数")?;
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;
    
    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
    Ok(lines.join("\n"))
})
```

### 5.5 参数传递与结果处理

工具执行流程（`agent/runtime.rs:139-202`）：

```rust
// 解析参数
let args: serde_json::Value = serde_json::from_str(&tc.arguments)
    .unwrap_or(serde_json::Value::Null);

// 执行工具
let tool_result = match self.tool_registry.execute(&tc.name, args).await {
    Ok(result) => {
        // 截断过长结果（最大 8000 字符）
        let truncated = if result.len() > 8000 {
            format!("{}...\n\n[结果已截断，原长度: {} 字符]", &result[..8000], result.len())
        } else {
            result
        };
        truncated
    }
    Err(e) => {
        format!("工具执行错误: {}", e)
    }
};

// 推入工具结果消息
self.session.push_tool_result(&tc.id, &tc.name, &tool_result);
```

---

## 6. 记忆系统

### 6.1 存储结构设计

记忆存储在 `memory/store.rs` 中实现：

```rust
/// 记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub content: String,       // 记忆内容
    pub category: String,      // 分类标签
    pub added_at: String,      // 添加时间
    pub last_accessed: String, // 最后访问时间
    pub access_count: u32,     // 访问次数
}

/// 记忆存储
pub struct MemoryStore {
    memory_path: PathBuf,  // MEMORY.md (JSONL 格式)
    user_path: PathBuf,    // USER.md (用户档案)
}
```

### 6.2 读写机制

**添加记忆**（`memory/store.rs:39-60`）：

```rust
pub fn add_memory(&self, content: &str, category: &str) -> Result<(), AppError> {
    let memory = MemoryEntry {
        content: content.to_string(),
        category: category.to_string(),
        added_at: chrono::Utc::now().to_rfc3339(),
        last_accessed: chrono::Utc::now().to_rfc3339(),
        access_count: 0,
    };
    
    let line = serde_json::to_string(&memory)?;
    let mut existing = fs::read_to_string(&self.memory_path).unwrap_or_default();
    if !existing.is_empty() {
        existing.push('\n');
    }
    existing.push_str(&line);
    fs::write(&self.memory_path, existing)?;
    Ok(())
}
```

**查询记忆**（`memory/store.rs:62-94`）：

```rust
pub fn query_memories(&self, query: &str) -> Result<Vec<String>, AppError> {
    let content = fs::read_to_string(&self.memory_path).unwrap_or_default();
    let query_lower = query.to_lowercase();
    
    let mut matches: Vec<String> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<MemoryEntry>(line).ok())
        .filter(|entry| {
            query.is_empty()
                || entry.content.to_lowercase().contains(&query_lower)
                || entry.category.to_lowercase().contains(&query_lower)
        })
        .map(|entry| format!("[{}] {} ({})", entry.category, entry.content, entry.added_at))
        .collect();
    
    // 限制返回数量
    if matches.len() > 10 {
        matches.truncate(10);
    }
    
    Ok(matches)
}
```

### 6.3 记忆检索算法

采用 **简单字符串匹配** 策略：
- 支持按内容或分类搜索
- 不区分大小写
- 结果按时间排序（最新优先）

### 6.4 过期清理机制

当前实现中访问计数更新被简化（`memory/store.rs:149-151`）：

```rust
fn update_access_counts(&self, _query: &str) -> Result<(), AppError> {
    // 简化实现：不更新文件中的访问计数（避免频繁写入）
    Ok(())
}
```

---

## 7. 会话管理系统

### 7.1 会话数据结构

会话存储在 `storage.rs` 中实现：

```rust
/// 会话数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,                 // 会话唯一标识 (UUID)
    pub name: String,               // 会话名称
    pub created_at: String,         // 创建时间
    pub updated_at: String,         // 更新时间
    pub model: String,              // 使用的模型
    pub metadata: Option<String>,   // 附加元数据
}

/// 消息数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,                 // 消息唯一标识
    pub session_id: String,         // 所属会话 ID
    pub role: String,               // 角色: user/assistant/tool
    pub content: String,            // 消息内容
    pub created_at: String,         // 创建时间
    pub metadata: Option<String>,   // 附加元数据
}
```

### 7.2 文件存储布局

```
data_dir/
├── {session_id}.json      # 会话元数据
└── messages/
    └── {session_id}.jsonl # 消息历史（JSONL 格式）
```

### 7.3 会话创建与恢复

**创建会话**（`storage.rs:78-93`）：

```rust
pub fn create_session(&self, name: &str, model: Option<&str>) -> Result<Session, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    
    let session = Session {
        id: id.clone(),
        name: name.to_string(),
        created_at: now.clone(),
        updated_at: now,
        model: model.unwrap_or("gpt-4").to_string(),
        metadata: None,
    };
    
    self.save_session_file(&session)?;
    Ok(session)
}
```

**获取会话**（`storage.rs:96-104`）：

```rust
pub fn get_session(&self, id: &str) -> Result<Session, AppError> {
    let path = self.session_path(id);
    if !path.exists() {
        return Err(AppError::NotFound(format!("会话不存在: {}", id)));
    }
    let content = fs::read_to_string(&path)?;
    let session = serde_json::from_str::<Session>(&content)?;
    Ok(session)
}
```

### 7.4 消息存储（JSONL 格式）

消息采用 **JSONL（JSON Lines）** 格式存储，支持增量写入：

```rust
pub fn append_message(&self, session_id: &str, message: &Message) -> Result<(), AppError> {
    let path = self.messages_path(session_id);
    let line = serde_json::to_string(message)?;
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    content.push_str(&line);
    content.push('\n');
    fs::write(&path, content)?;
    
    // 更新会话时间戳
    let now = chrono::Utc::now().to_rfc3339();
    if let Ok(mut session) = self.get_session(session_id) {
        session.updated_at = now;
        self.save_session_file(&session)?;
    }
    
    Ok(())
}
```

### 7.5 资源释放策略

**删除会话**（`storage.rs:107-121`）：

```rust
pub fn delete_session(&self, id: &str) -> Result<(), AppError> {
    // 删除会话元数据文件
    let session_path = self.session_path(id);
    if session_path.exists() {
        fs::remove_file(&session_path)?;
    }
    
    // 删除消息文件
    let msg_path = self.messages_path(id);
    if msg_path.exists() {
        fs::remove_file(&msg_path)?;
    }
    
    Ok(())
}
```

---

## 8. Skills 技能系统

### 8.1 技能定义规范

技能定义在 `skills/loader.rs` 中实现：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    pub name: String,        // 技能名称
    pub description: String, // 技能描述
    pub version: String,     // 版本号
    pub content: String,     // 技能指令内容
    pub source_path: String, // 来源路径
    pub enabled: bool,       // 是否启用
}
```

### 8.2 技能文件格式（SKILL.md）

```markdown
---
name: 代码审查
description: 帮助审查代码质量和安全性
version: 1.0.0
---

## 代码审查技能

你是一个专业的代码审查助手。请按照以下步骤进行代码审查：

1. 检查代码是否符合最佳实践
2. 识别潜在的安全漏洞
3. 提供改进建议
...
```

### 8.3 技能加载机制

```rust
pub fn list_skills(&self) -> Vec<SkillDef> {
    let mut skills = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&self.skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() { continue; }
            
            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                if let Some(skill) = Self::parse_skill_md(&content, &path) {
                    skills.push(skill);
                }
            }
        }
    }
    
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}
```

### 8.4 技能解析实现

```rust
fn parse_skill_md(content: &str, dir_path: &PathBuf) -> Option<SkillDef> {
    let mut name = String::new();
    let mut description = String::new();
    let mut version = String::from("0.1.0");
    let mut in_frontmatter = false;
    let mut body_start = 0;
    
    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if i == 0 && line == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter && line == "---" {
            in_frontmatter = false;
            body_start = i + 1;
            continue;
        }
        if in_frontmatter {
            if let Some(value) = line.strip_prefix("name:") {
                name = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("description:") {
                description = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("version:") {
                version = value.trim().to_string();
            }
        }
    }
    
    if name.is_empty() { return None; }
    
    let body_content: String = content
        .lines()
        .skip(body_start)
        .collect::<Vec<&str>>()
        .join("\n");
    
    Some(SkillDef {
        name,
        description,
        version,
        content: body_content,
        source_path: dir_path.display().to_string(),
        enabled: true,
    })
}
```

### 8.5 技能扩展机制

技能通过 **文件系统目录** 进行扩展：
1. 在 `skills/` 目录下创建新目录
2. 创建 `SKILL.md` 文件
3. 系统自动发现并加载

---

## 9. LLM 客户端与通信协议

### 9.1 API 客户端实现

`LlmClient` 在 `llm/client.rs` 中实现，支持标准和流式两种模式：

```rust
pub struct LlmClient {
    http: Client,
    provider: Arc<ProviderConfig>,
    timeout_secs: u32,
}
```

### 9.2 Base URL 标准化

针对本地服务（如 LM Studio/Ollama）的特殊处理：

```rust
pub fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    // 如果已含 /v1/ 或以 /v1 结尾，直接返回
    if trimmed.ends_with("/v1") || trimmed.contains("/v1/") {
        return trimmed.to_string();
    }
    // 自动追加 /v1（兼容 LM Studio / Ollama）
    if trimmed.contains("localhost") || trimmed.contains("127.0.0.1") {
        return format!("{}/v1", trimmed);
    }
    trimmed.to_string()
}
```

### 9.3 错误处理增强

针对非标准响应的处理（`llm/client.rs:80-88`）：

```rust
// 检查响应体是否包含非标准的 "error" 字段
if let Ok(err_val) = serde_json::from_str::<serde_json::Value>(&body) {
    if let Some(err_msg) = err_val.get("error") {
        let msg = err_msg.as_str().unwrap_or("未知错误");
        return Err(AppError::LlmError(format!(
            "服务端返回错误: {}\n\n提示: 请确认 base_url 配置正确",
            msg
        )));
    }
}
```

### 9.4 流式响应处理

流式聊天实现（`llm/client.rs:98-233`）：

```rust
pub async fn chat_stream(&self, req: &ChatRequest) -> Result<mpsc::Receiver<StreamEvent>, AppError> {
    let (tx, rx) = mpsc::channel(256);
    
    // 在独立任务中解析 SSE 流
    tokio::spawn(async move {
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&chunk_str);
                    
                    while let Some(pos) = buffer.find("\n\n") {
                        let line_block = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();
                        
                        // 解析 SSE 事件...
                    }
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(format!("流读取错误: {}", e))).await;
                    return;
                }
            }
        }
        
        let _ = tx.send(StreamEvent::Done("done".to_string())).await;
    });
    
    Ok(rx)
}
```

---

## 10. 配置系统

### 10.1 配置结构

```rust
pub struct AppConfig {
    pub port: u16,                          // HTTP 端口
    pub host: String,                       // 监听地址
    pub providers: Vec<ProviderConfig>,     // LLM 提供商列表
    pub default_model: String,              // 默认模型
    pub llm_timeout: u32,                   // LLM 请求超时（秒）
    pub max_retries: u32,                   // 最大重试次数
    pub max_iterations: usize,              // 最大 Agent 迭代次数
    pub temperature: f64,                   // 温度参数
    pub allowed_origins: Vec<String>,       // CORS 允许来源
    pub prompt_injection_protection: bool,  // 提示注入保护
    pub data_dir: Option<String>,           // 数据目录（可选）
}
```

### 10.2 平台自适应路径

`config.rs:166-208` 实现了平台自适应的数据目录：

| 平台 | 路径示例 |
|------|----------|
| Windows | `%LOCALAPPDATA%\NovaClaw\` |
| macOS | `~/Library/Application Support/NovaClaw/` |
| Linux | `$XDG_DATA_HOME/NovaClaw/` 或 `~/.local/share/NovaClaw/` |

---

## 11. 模块间交互关系

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         模块交互关系图                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐      │
│   │   Server    │         │   Agent     │         │    LLM      │      │
│   │  (Axum/Tau) │         │   Runtime   │         │   Client    │      │
│   └──────┬──────┘         └──────┬──────┘         └──────┬──────┘      │
│          │                       │                       │              │
│          ▼                       ▼                       ▼              │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐      │
│   │  Sessions   │◀────────│  Session    │         │   Config    │      │
│   │   Routes    │         │  (会话状态)  │         │  (配置加载)  │      │
│   └─────────────┘         └──────┬──────┘         └─────────────┘      │
│                                  │                                     │
│                                  ▼                                     │
│                         ┌─────────────┐                                │
│                         │   Tools     │                                │
│                         │  Registry   │                                │
│                         └──────┬──────┘                                │
│                                │                                       │
│          ┌─────────────────────┼─────────────────────┐                 │
│          ▼                     ▼                     ▼                 │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐             │
│   │  Builtin    │     │   Memory    │     │   Skills    │             │
│   │  Tools      │     │   Store     │     │   Loader    │             │
│   └─────────────┘     └─────────────┘     └─────────────┘             │
│                                                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 12. 设计模式与最佳实践

### 12.1 采用的设计模式

| 模式 | 应用场景 | 示例位置 |
|------|----------|----------|
| **依赖注入** | LlmClient、ToolRegistry 通过参数注入 | `agent/runtime.rs:41-56` |
| **观察者模式** | 步骤事件通过 mpsc 通道广播 | `agent/runtime.rs:126-137` |
| **策略模式** | 不同工具通过 Handler 执行 | `tools/registry.rs:14-64` |
| **建造者模式** | SystemPromptBuilder 链式构建 | `agent/prompt.rs:34-63` |
| **单例模式** | APP_STATE 全局状态管理 | `lib.rs` |

### 12.2 最佳实践

1. **线程安全**：使用 `Arc<RwLock<T>>` 保护共享状态
2. **异步优先**：全系统采用 Tokio 异步运行时
3. **错误处理**：统一使用 `AppError` 枚举和 `anyhow::Result`
4. **日志追踪**：使用 `tracing` 进行结构化日志
5. **配置热加载**：支持配置文件自动重新加载
6. **平台适配**：通过 `cfg!` 宏实现跨平台路径处理

---

## 附录：关键文件清单

| 文件路径 | 功能描述 |
|----------|----------|
| `src/agent/runtime.rs` | ReAct 循环核心实现 |
| `src/agent/session.rs` | 会话状态管理 |
| `src/agent/prompt.rs` | 系统提示词构建 |
| `src/agent/cot.rs` | 思维链提取 |
| `src/tools/registry.rs` | 工具注册中心 |
| `src/tools/builtin.rs` | 内置工具实现 |
| `src/memory/store.rs` | 记忆存储系统 |
| `src/skills/loader.rs` | 技能加载器 |
| `src/storage.rs` | 会话持久化存储 |
| `src/llm/client.rs` | LLM API 客户端 |
| `src/config.rs` | 配置管理 |
| `src/server/ws/chat.rs` | WebSocket 实时聊天 |
