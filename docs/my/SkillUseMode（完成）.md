# 技能使用模式切换指南

当前使用 **预注入模式**（所有技能名称+描述自动注入系统提示词），LLM 在系统提示词的 `<available_skills>` 中直接看到所有可用技能，通过 `skill_view(name)` 加载详细内容。

> `skills_list` 工具已删除，预注入模式下不再需要。

如需切回 **on-demand 模式**（LLM 通过工具发现技能），按以下步骤操作。

---

## 切换为 on-demand 模式

### 1. `backend/src/agent/prompt.rs` — 替换技能索引注入

**修改 `build_skill_index()` 方法：**

```diff
     fn build_skill_index(&self) -> String {
-        let mut index = String::from("# Available Skills\n\n");
-        index.push_str("Before responding, scan the following skills. If a skill matches or is partially relevant to your task, use skill_view(name) to load and follow its instructions.\n\n");
-        index.push_str("<available_skills>\n");
-
-        for skill in &self.skill_list {
-            index.push_str(&format!("  - {}\n", skill));
-        }
-
-        index.push_str("</available_skills>\n");
-        index
+        r#"# Available Skills
+
+Skill tools can extend your capabilities beyond built-in tools.
+- **Before responding**: check if the user's request might be handled by a skill.
+- If built-in tools cannot fully satisfy the request, call **skills_list** first to discover available skills.
+- Once you find a matching skill, call **skill_view(name)** to load its full instructions and follow them.
+- The skill_view response includes a "skill_dir" field — use it to reference scripts or assets in that skill.
+- Supports {SKILL_DIR}, ${SKILL_DIR}, ${HERMES_SKILL_DIR} placeholders in skill content (auto-resolved to absolute paths)."#.to_string()
     }
```

**修改 `build()` 方法：**

```diff
     // 6. 技能索引层（预注入模式：有技能列表时才注入）
-    if !self.skill_list.is_empty() {
-        sections.push(self.build_skill_index());
-    }
+    sections.push(self.build_skill_index());
```

**移除 `skill_list` 字段和 `with_skills()` 方法：**

```diff
 pub struct SystemPromptBuilder<'a> {
     #[allow(dead_code)]
     config: &'a AppConfig,
     os_name: String,
     workspace: Option<String>,
-    skill_list: Vec<String>,
 }

 impl<'a> SystemPromptBuilder<'a> {
     pub fn new(
         config: &'a AppConfig,
         os_name: impl Into<String>,
         workspace: Option<impl Into<String>>,
     ) -> Self {
         Self {
             config,
             os_name: os_name.into(),
             workspace: workspace.map(|s| s.into()),
-            skill_list: Vec::new(),
         }
     }

-    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
-        self.skill_list = skills;
-        self
-    }
```

### 2. `backend/src/agent/runtime.rs` — 移除技能列表传递

**修改 `build_system_prompt()` 方法：**

```diff
     crate::agent::prompt::SystemPromptBuilder::new(
         &self.config,
         os_name,
         self.session.workspace.as_deref(),
     )
-    .with_skills(self.skills.iter().map(|s| {
-        format!("{}: {}", s.name, s.description)
-    }).collect())
     .build()
```

**移除 `skills` 字段和构造函数参数：**

```diff
 pub struct AgentRuntime {
     session: AgentSession,
     llm_client: LlmClient,
     tool_registry: Arc<ToolRegistry>,
     config: AppConfig,
     max_iterations: usize,
     max_retries: u32,
     has_first_reasoning: bool,
     accumulated_again_reasonings: Vec<String>,
-    skills: Vec<SkillDef>,
     executed_tools: HashSet<String>,
     tool_retry_count: HashMap<String, u32>,
 }
```

```diff
- use crate::skills::loader::SkillDef;
```

```diff
     pub fn new(
         session: AgentSession,
         llm_client: LlmClient,
         tool_registry: Arc<ToolRegistry>,
         config: &AppConfig,
-        skills: Vec<SkillDef>,
     ) -> Self {
         Self {
             // ...
-            skills,
             executed_tools: HashSet::new(),
             tool_retry_count: HashMap::new(),
         }
     }
```

### 3. 移除调用方

**`backend/src/server/routes/chat.rs`** — 两处移除 skills 参数。

**`backend/src/cron.rs`** — 移除 `let skills = Vec::new();` 和 skills 参数。

### 4. 恢复 `skills_list` 工具

在 `backend/src/tools/builtin.rs` 中重新注册 `skills_list` 工具，参考删除前的代码模式。
