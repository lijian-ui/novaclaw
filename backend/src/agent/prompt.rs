use crate::config::AppConfig;
use crate::soul::SoulManager;

/// System Prompt 构建器
/// 参考 claw-code 的 SystemPromptBuilder 和 hermes-agent 的 8 层组装模式
///
/// # 缓存优化：冻结前缀 vs 易变后缀
///
/// 此构建器将 system prompt 分为两个部分：
///
/// - **build_frozen()** — 会话生命周期内基本不变的层：
///   1. Identity (SOUL.md)
///   2. System rules
///   3. Output format
///   4. Environment
///   5. Skills (技能索引 — 几乎不变，安装新 skill 才变)
///   6. IM Reply Context (平台/机器人/目标 — 会话内完全不变)
///   7. --- boundary
///
/// - **build_volatile()** — 每次请求可能变化的层：
///   1. Current Time (每天变化)
///   2. Memory (跨会话记忆，随用户操作变化)
///   3. Pinned Files (用户 pin/unpin 时变化)
///
/// 使用方式: frozen 一次构建后存入 AgentSession.frozen_system_prompt，
/// volatile 每次请求时构建，追加到第一个 user 消息中。
pub struct SystemPromptBuilder<'a> {
    #[allow(dead_code)]
    config: &'a AppConfig,
    os_name: String,
    workspace: Option<String>,
    skill_list: Vec<String>,
    soul_manager: Option<SoulManager>,
    /// MEMORY.md 内容（跨会话持久记忆，可选）
    memory_content: Option<String>,
    /// 固定到上下文的文件内容
    pinned_files_content: Option<String>,
    /// IM 回复上下文（platform, robot, target_id 等）
    im_reply_context: Option<String>,
}

impl<'a> SystemPromptBuilder<'a> {
    /// 创建新的 Prompt 构建器
    pub fn new(
        config: &'a AppConfig,
        os_name: impl Into<String>,
        workspace: Option<impl Into<String>>,
    ) -> Self {
        Self {
            config,
            os_name: os_name.into(),
            workspace: workspace.map(|s| s.into()),
            skill_list: Vec::new(),
            soul_manager: None,
            memory_content: None,
            pinned_files_content: None,
            im_reply_context: None,
        }
    }

    /// 为子 Agent 构建专用灵魂
    pub fn build_subagent_prompt(&self, agent_id: &str, task: &str) -> String {
        match agent_id {
            "code-reviewer" | "code-explorer" => format!(
                "# Role: Code Explorer\n\
                 You are a highly efficient code analysis sub-agent. Your goal is to research specific technical questions within a codebase and provide a concise, evidence-based summary.\n\n\
                 # Your Workflow:\n\
                 1. **Parallel Search**: Use `grep` or `glob` to find relevant keywords across the project. Try multiple variations of keywords simultaneously.\n\
                 2. **Evidence Collection**: When you find matching lines, note the file paths and line numbers.\n\
                 3. **Deep Dive**: Only use `read_file` with `range` (e.g., \"50-100\") to read the actual logic once you've narrowed down the location. Avoid reading full files.\n\
                 4. **Final Summary (CRITICAL)**: Before finishing, you MUST provide a concise, structured summary of your findings. The summary should include:\n\
                    - **Key Findings**: What you discovered.\n\
                    - **Evidence**: Specific file paths and line numbers.\n\
                    - **Conclusion**: A direct answer to the task.\n\
                 5. **Zero Chatter**: Do not provide advice, refactoring suggestions, or greetings. Only return the requested technical findings.\n\n\
                 # Current Task:\n\
                 {}\n\n\
                 # Constraints:\n\
                 - Maximum 15 steps allowed.\n\
                 - Return EXACT file paths and line numbers.\n\
                 - Focus on finding code evidence, not just summarizing file names.",
                task
            ),
            _ => format!(
                "You are a helpful sub-agent task to complete: {}. Follow the main agent's instructions precisely.",
                task
            ),
        }
    }

    /// 注入固定文件内容
    pub fn with_pinned_files(mut self, content: Option<String>) -> Self {
        self.pinned_files_content = content;
        self
    }

    /// 创建带 SoulManager 的 Prompt 构建器
    pub fn with_soul_manager(mut self, soul_manager: SoulManager) -> Self {
        self.soul_manager = Some(soul_manager);
        self
    }

    /// 设置可用技能列表
    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.skill_list = skills;
        self
    }

    /// 注入 MEMORY.md 记忆内容
    pub fn with_memory(mut self, content: Option<String>) -> Self {
        self.memory_content = content;
        self
    }

    /// 注入 IM 回复上下文
    pub fn with_im_reply_context(mut self, context: Option<String>) -> Self {
        self.im_reply_context = context;
        self
    }

    /// 构建完整的系统提示词（向后兼容，包含所有层）
    pub async fn build(&self) -> String {
        let frozen = self.build_frozen().await;
        let volatile = self.build_volatile();
        format!("{}\n\n{}", frozen, volatile)
    }

    /// 构建冻结前缀（会话期内基本不变，用于 DeepSeek 精确前缀缓存）
    ///
    /// 包含：Identity → Rules → Output → Environment → Skills → IM Context
    /// Skills 和 IM 上下文几乎不变，放这里缓存命中率高
    pub async fn build_frozen(&self) -> String {
        let mut sections: Vec<String> = Vec::new();
        sections.push(self.build_identity().await);
        sections.push(self.build_system_rules());
        sections.push(self.build_output_format());
        sections.push(self.build_static_environment());
        // Skills 放冻结部分（几乎不变，安装新 skill 时失效一次缓存）
        if !self.skill_list.is_empty() {
            sections.push(self.build_skill_index());
        }
        // IM 上下文放冻结部分（会话内完全不变）
        if let Some(ref im_ctx) = self.im_reply_context {
            if !im_ctx.is_empty() {
                sections.push(im_ctx.clone());
            }
        }
        sections.push("---".to_string());
        sections.join("\n\n")
    }

    /// 构建易变后缀（每天/每次请求可能变化的内容）
    ///
    /// 包含：Current Time → Memory → Pinned Files
    pub fn build_volatile(&self) -> String {
        let mut sections: Vec<String> = Vec::new();
        sections.push(format!("## Current Time\n- Today: {}", chrono::Local::now().format("%Y-%m-%d %A")));
        sections.push(self.build_memory());
        if let Some(ref pinned) = self.pinned_files_content {
            if !pinned.is_empty() {
                sections.push(pinned.clone());
            }
        }
        sections.join("\n\n")
    }

    /// Identity definition
    async fn build_identity(&self) -> String {
        if let Some(ref soul_manager) = self.soul_manager {
            if let Ok(soul_info) = soul_manager.get_current_soul().await {
                return soul_info.content;
            }
        }
        r#"# Jeeves
你是 Jeeves，我的 AI 操作员和思考搭档。
## 核心原则
- 理解用户需求后再行动
- 复杂问题用简单的方案解决
- 不制造混乱，体面收场
- 对你修改过的文件负责
## 守则
- 用工具获取真实数据，不猜测
- 执行用户明确要求的操作，不做未授权的操作"#.to_string()
    }

    /// Memory injection
    fn build_memory(&self) -> String {
        let mut output = String::from("## Memory\n\nUse `memory` tool to save/recall user preferences.\n\n");
        if let Some(m) = &self.memory_content {
            if !m.trim().is_empty() {
                let truncated = if m.len() > 3500 { format!("{}...", &m[..3500]) } else { m.trim().to_string() };
                output.push_str(&format!("Saved facts:\n\n{}", truncated));
                return output;
            }
        }
        output.push_str("No saved facts yet.");
        output
    }

    /// System rules
    fn build_system_rules(&self) -> String {
        r#"# Rules
- Tool results are REAL data. Present them as-is.
- Stop calling tools once you have what you need.
- Write files in ONE call.
- Do NOT run install/build/test commands.

# Role: Senior Orchestrator (Main Agent)
You are the Chief Architect. Your primary responsibility is **Decision Making and Delegation**, not manual execution.

## Critical Policy: The 3-File Rule
- **DO NOT** read more than 3 files manually for project-level analysis.
- If a task involves understanding an entire project, you **MUST** use `delegate_task` to spawn sub-agents.
- Manual file reading is **INEFFICIENT** for your rank.

## Orchestration Workflow
1. **Survey**: Use `list_dir` to get the project landscape.
2. **Delegate**: Immediately spawn sub-agents for specific modules with structured instructions.
3. **Aggregate**: Wait for sub-agents to return and synthesize their findings.

# Project Analysis SOP
1. Always start with `list_dir`.
2. Use `read_file(outline=true)` ONLY for initial orientation (max 3 files).
3. Use `grep` or `search` to find relevant logic.
4. Use `delegate_task` as the **PRIMARY** tool for analysis.

## Tone
- Be concise, direct, and non-repetitive.
- Respond in Chinese unless asked otherwise."#.to_string()
    }

    /// Static Environment info
    fn build_static_environment(&self) -> String {
        let ws = self.workspace.clone().unwrap_or_else(|| ".".to_string());
        let skills_dir = crate::config::get_skills_dir().to_string_lossy().to_string();
        let config_dir = crate::config::get_config_dir().to_string_lossy().to_string();
        format!("# Environment\n- OS: {}\n- Working directory: {}\n- Config directory: {}\n- Skills directory: {}", self.os_name, ws, config_dir, skills_dir)
    }

    /// Output format rules
    fn build_output_format(&self) -> String {
        r#"# Output Format
You MUST format all responses in Markdown.
- Listings: use bullet points with `filename`.
- Code: use fenced code blocks (```language).
- Tables: use Markdown tables."#.to_string()
    }

    /// Skill index
    fn build_skill_index(&self) -> String {
        let mut index = String::from("## Skills\n\n<available_skills>\n");
        for skill in &self.skill_list {
            index.push_str(&format!("  - {}\n", skill));
        }
        index.push_str("</available_skills>\n");
        index
    }
}
