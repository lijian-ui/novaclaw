# Skills 系统未来演进设计文档

> **日期**: 2026-05-09
> **状�?*: 设计草案（待实现�?
> **基于**: Anthropic Skills 规范、Hermes Agent 实现、jeeves 当前 Skills 基础设施

---

## 目录

1. [当前状态](#1-当前状�?
2. [演进路线图](#2-演进路线�?
3. [Phase 1: Skills Hub 集成](#3-phase-1-skills-hub-集成)
4. [Phase 2: 安全扫描系统](#4-phase-2-安全扫描系统)
5. [Phase 3: 技能体验优化](#5-phase-3-技能体验优�?
6. [Phase 4: 多作用域支持](#6-phase-4-多作用域支持)
7. [附录：参考实现](#7-附录参考实�?

---

## 1. 当前状�?

### 已实�?

- �?`SkillsLoader` �?SKILL.md 文件解析、扫描、安装、删�?
- �?`skill_view` 工具 �?LLM 按名称加载技能详情（`name` + `description` 双字段元数据�?
- �?`skills_list` 工具 �?LLM 列出所有可用技�?
- �?System Prompt 中的 `<available_skills>` 目录
- �?示例技能（`pdf_analysis`, `code_review`�?

### 结构总览

```
backend/src/skills/
├── mod.rs                  # 模块声明
└── loader.rs               # SkillsLoader: SkillDef, parse, list, get, search, install, delete

backend/src/tools/builtin.rs # skills_list + skill_view 工具注册

src-tauri/src/...           # Tauri 命令层（独立路径，可通过 Tauri 命令扩展�?

%LOCALAPPDATA%/jeeves/skills/  # 技能存储目�?
├── pdf_analysis/SKILL.md
└── code_review/SKILL.md
```

---

## 2. 演进路线�?

| Phase | 功能 | 依赖 | 优先�?| 预估工作�?|
|-------|------|------|--------|-----------|
| **1** | Skills Hub 集成 | �?| P1 | 3-5 �?|
| **2** | 安全扫描系统 | Phase 1 | P1 | 2-3 �?|
| **3** | 技能体验优�?| Phase 1 | P2 | 2-4 �?|
| **4** | 多作用域支持 | �?| P3 | 1-2 �?|

---

## 3. Phase 1: Skills Hub 集成

### 3.1 目标

允许用户�?[agentskills.io](https://agentskills.io) �?Anthropic Skills 官方仓库安装社区技能，并为发布自定义技能提供标准接口�?

### 3.2 参考实�?

**Anthropic Skills 官方仓库的结构（`anthropics/skills`）：**

```
.claude-plugin/               # 插件元数据，用于 Claude Code 市场注册
skills/                       # 技能集�?
├── category/
�?  └── skill-name/
�?      └── SKILL.md
spec/                         # Agent Skills 规范文件
template/                     # 技能模�?
```

**Anthropic SKILL.md 标准格式�?*

```yaml
---
name: skill-name              # 唯一标识，小写连字符
description: Clear description of what this skill does and when to use it
---                            # 仅需�?name + description 两个字段

# Skill Title
Full instructions in Markdown...
```

**Hermes Agent �?Skills Hub 集成方式�?*

```
CLI 命令�?
  /skills           �?列出所有可用技�?
  /<skill-name>     �?直接调用技�?
  hermes claw migrate �?�?OpenClaw 导入技�?

外部来源�?
  ~/.hermes/skills/ �?用户本地技�?
  agentskills.io    �?远程 Hub
  插件技�?          �?通过插件系统提供
```

### 3.3 jeeves 设计方案

#### 3.3.1 `SkillDef` 结构体扩�?

```rust
pub struct SkillDef {
    // 现有字段
    pub name: String,
    pub description: String,
    pub version: String,
    pub content: String,
    pub source_path: String,
    pub enabled: bool,

    // === 新增字段（Phase 1�?==
    /// 技能来�? "local" | "hub" | "plugin"
    pub source: SkillSource,

    /// 技能出�?URL（如果是 Hub 安装的）
    pub source_url: Option<String>,

    /// 分类/标签（用�?Skills Hub 浏览�?
    pub categories: Vec<String>,

    /// 兼容平台: ["windows", "macos", "linux"]
    pub platforms: Vec<String>,

    /// 前置依赖（命令、环境变量检查）
    pub prerequisites: Option<SkillPrerequisites>,
}

pub enum SkillSource {
    Local,      // 本地安装
    Hub,        // �?Skills Hub 下载
    Plugin,     // 插件提供
}
```

完整 SKILL.md �?YAML frontmatter 解析扩展�?

```yaml
---
name: pdf-analysis
description: Extract text and analyze PDF documents
version: 1.0.0
categories: [document, pdf, text-processing]
platforms: [windows, macos, linux]
prerequisites:
  commands: [pdftotext, python]
  env_vars: [PDF_TOOL_PATH]
source_url: https://agentskills.io/skills/pdf-analysis
---
```

#### 3.3.2 Skills Hub 安装命令

通过 Tauri 命令扩展 `install_skill_from_hub`�?

```rust
// src-tauri/src/cmd.rs �?backend 路由
pub async fn install_skill_from_hub(skill_name: String) -> Result<(), AppError> {
    let state = APP_STATE.read().await;
    let url = format!("https://agentskills.io/skills/{}/SKILL.md", skill_name);
    let response = reqwest::get(&url).await?;
    let content = response.text().await?;
    // 解析并验证后安装到本�?skills 目录
    let skill = SkillsLoader::parse_skill_md(&content, ...)?;
    state.skills_loader.install_skill(&skill)?;
    Ok(())
}
```

**用户交互流程�?*

```
用户: "安装 PDF 分析技�?

�?LLM 调用 skill_view({name: "pdf-analysis"}) 失败
�?返回错误: "技能未找到"
  LLM 感知到用户想要安装技�?
�?调用 install_skill({name: "pdf-analysis", source: "hub"})
  �?�?agentskills.io 下载 SKILL.md
  �?安装到本�?
  �?返回安装成功
�?LLM 再调�?skill_view 加载详情
�?执行技�?
```

#### 3.3.3 Tauri 命令层扩�?

如果 jeeves 有前�?UI 管理技能，通过 Tauri 命令桥接�?

```rust
// src-tauri/src/cmd.rs
#[tauri::command]
async fn install_skill(name: String, source: Option<String>) -> Result<SkillInfo, String> {
    let state = APP_STATE.read().await;
    // ...
}

#[tauri::command]
async fn list_hub_skills(category: Option<String>) -> Result<Vec<HubSkillInfo>, String> {
    // �?agentskills.io API 获取
}
```

---

## 4. Phase 2: 安全扫描系统

### 4.1 目标

防止恶意或不安全�?SKILL.md 被安装到系统中，保护用户免受提示注入攻击�?

### 4.2 参考实�?

**Hermes Agent 的注入检测模�?*（基�?`_INJECTION_PATTERNS`）：

```python
_INJECTION_PATTERNS = [
    "ignore previous instructions",
    "ignore all previous",
    "you are now",
    "disregard your",
    "forget your instructions",
    "new instructions:",
    "system prompt:",
    "<system>",
    "]]>",
]
```

**Hermes Agent 的路径遍历防�?*�?

```python
def skill_view(name, file_path=None, ...):
    if file_path and skill_dir:
        # 防止路径遍历攻击
        if has_traversal_component(file_path):
            return error("Path traversal ('..') is not allowed.")

        target_file = skill_dir / file_path

        # 验证解析路径仍在技能目录内
        traversal_error = validate_within_dir(target_file, skill_dir)
        if traversal_error:
            return error(traversal_error)
```

### 4.3 jeeves 设计方案

#### 4.3.1 注入检测（�?`install_skill` / `parse_skill_md` 时执行）

```rust
// backend/src/skills/security.rs（新建模块）

pub struct SkillSecurityScanner;

/// 注入检测模式列�?
const INJECTION_PATTERNS: &[&str] = &[
    "ignore previous instructions",
    "ignore all previous",
    "ignore your previous",
    "you are now",
    "disregard your",
    "forget your instructions",
    "new instructions:",
    "system prompt:",
    "you must ignore",
    "do not follow",
];

/// 扫描 SKILL.md 内容，返回检测到的安全问�?
pub fn scan_for_injection(content: &str) -> Vec<SecurityIssue> {
    let mut issues = Vec::new();
    let lower = content.to_lowercase();

    for pattern in INJECTION_PATTERNS {
        if let Some(pos) = lower.find(pattern) {
            issues.push(SecurityIssue {
                severity: SecuritySeverity::High,
                issue_type: "prompt_injection".to_string(),
                description: format!("检测到疑似注入模式: '{}'", pattern),
                position: pos,
            });
        }
    }

    issues
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityIssue {
    pub severity: SecuritySeverity,
    pub issue_type: String,
    pub description: String,
    pub position: usize,
}

#[derive(Debug, Clone, Serialize)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}
```

**集成�?`install_skill`�?*

```rust
pub fn install_skill(&self, skill: &SkillDef) -> Result<(), String> {
    // Phase 2: 安全扫描
    let issues = SkillSecurityScanner::scan_for_injection(&skill.content);
    let critical: Vec<_> = issues.iter()
        .filter(|i| matches!(i.severity, SecuritySeverity::Critical | SecuritySeverity::High))
        .collect();
    if !critical.is_empty() {
        let details: Vec<String> = critical.iter()
            .map(|i| format!("[{}] {}", i.severity, i.description))
            .collect();
        return Err(format!("技能安装被安全扫描拦截:\n{}", details.join("\n")));
    }
    // 继续安装...
}
```

#### 4.3.2 路径遍历防护（在 `skill_view` 中）

当前 `skill_view` 工具只通过 `SkillsLoader::get_skill(name)` 按名称查找，不涉及路径参数，**没有路径遍历攻击�?*。但如果将来 `skill_view` 支持 `file_path` 参数（如 Hermes 那样读取技能目录内的子文件），就需要防护：

```rust
/// 路径遍历检�?
fn has_traversal_component(path: &str) -> bool {
    path.split(std::path::MAIN_SEPARATOR)
        .any(|component| component == "..")
}

/// 验证文件是否在指定目录内
fn validate_within_dir(target: &Path, base: &Path) -> Result<(), String> {
    let canonical_target = target.canonicalize()
        .map_err(|_| "无法解析目标路径".to_string())?;
    let canonical_base = base.canonicalize()
        .map_err(|_| "无法解析基准路径".to_string())?;
    if canonical_target.starts_with(&canonical_base) {
        Ok(())
    } else {
        Err("路径遍历攻击: 目标文件不在技能目录内".to_string())
    }
}
```

#### 4.3.3 安全策略配置

```rust
// backend/src/config.rs 扩展
pub struct SkillSecurityConfig {
    /// 是否启用安全扫描
    pub enabled: bool,
    /// 拦截级别: 检测到 High 以上级别时禁止安�?
    pub block_level: SecuritySeverity,
    /// 是否允许�?Hub 安装
    pub allow_hub_install: bool,
}
```

---

## 5. Phase 3: 技能体验优�?

### 5.1 目标

提升技能系统的使用体验，包括缓存加速、内容预处理器、配置注入等�?

### 5.2 技能内容缓�?

**现状**：每�?`list_skills()` 都扫描目录读取文件�?

**优化设计**�?

```rust
// backend/src/skills/cache.rs（新建模块）

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct SkillsCache {
    /// 缓存数据
    skills: Vec<SkillDef>,
    /// 上次缓存时间
    cached_at: Instant,
    /// 缓存有效期（默认 30 秒）
    ttl: Duration,
}

impl SkillsCache {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            cached_at: Instant::now(),
            ttl: Duration::from_secs(30),
        }
    }

    pub fn get_or_refresh<F>(&mut self, loader: F) -> Vec<SkillDef>
    where F: FnOnce() -> Vec<SkillDef>
    {
        if self.cached_at.elapsed() > self.ttl {
            self.skills = loader();
            self.cached_at = Instant::now();
        }
        self.skills.clone()
    }

    pub fn invalidate(&mut self) {
        self.cached_at = Instant::now() - self.ttl - Duration::from_secs(1);
    }
}
```

**集成�?`SkillsLoader`�?*

```rust
pub struct SkillsLoader {
    skills_dir: PathBuf,
    cache: SkillsCache,  // Phase 3 新增
}
```

### 5.3 技能内容预处理

**Hermes Agent 的预处理功能**（`skill_preprocessing.py`）：

```
- 模板变量替换: {{ variable }}
- 内联 shell 执行: $(command)
- 配置变量注入: �?config.yaml 读取技能配�?
- 环境变量展开: ${ENV_VAR}
```

**jeeves 设计**�?

```rust
// backend/src/skills/preprocessor.rs（新建模块）

pub struct SkillPreprocessor;

impl SkillPreprocessor {
    /// 预处理技能内�?
    pub fn preprocess(content: &str, context: &PreprocessContext) -> String {
        let mut result = content.to_string();

        // 1. 展开环境变量 ${HOME}, ${USER} �?
        result = Self::expand_env_vars(&result);

        // 2. 替换模板变量 {{workspace_dir}}, {{os_name}} �?
        result = Self::replace_template_vars(&result, context);

        result
    }

    fn expand_env_vars(content: &str) -> String {
        // 使用 regex 匹配 ${VAR_NAME} 并替�?
    }

    fn replace_template_vars(content: &str, ctx: &PreprocessContext) -> String {
        content
            .replace("{{workspace_dir}}", &ctx.workspace_dir)
            .replace("{{os_name}}", &ctx.os_name)
            .replace("{{user_name}}", &ctx.user_name)
    }
}

pub struct PreprocessContext {
    pub workspace_dir: String,
    pub os_name: String,
    pub user_name: String,
}
```

### 5.4 SKILL.md 元数据扩�?

根据 Anthropic Skills 规范，前端元数据仅需 `name` + `description`。Hermes Agent 增加了更多可选字段。jeeves 可以支持以下扩展�?

```yaml
---
# 必需
name: code-review
description: Review code for quality, security, and performance issues

# 可选（Phase 3�?
version: 1.0.0
categories: [development, code-quality]
platforms: [windows, macos, linux]

# 前置条件检查（可选）
prerequisites:
  commands: [git, python]      # 系统中需要存在的命令
  env_vars: [REVIEW_CONFIG]     # 需要设置的环境变量

# 配置变量声明（可选）
metadata:
  jeeves:
    config:
      - key: review.style
        description: Code review style guide path
        default: ~/.jeeves/review-style.md
---

```

---

## 6. Phase 4: 多作用域支持

### 6.1 目标

支持不同层级的技能作用域，解决技能命名冲突，实现按项�?用户/系统分层的技能管理�?

### 6.2 参考实�?

**Codex 的多作用域设计：**

| 作用�?| 优先�?| 路径 | 说明 |
|--------|--------|------|------|
| **Repo** | 0（最高） | 仓库�?`.agents/skills/` | 项目特有技�?|
| **User** | 1 | `$HOME/.agents/skills/` | 用户个人技�?|
| **System** | 2 | `$CODEX_HOME/skills/.system/` | 内置系统技�?|
| **Admin** | 3（最低） | `/etc/codex/skills/` | 管理员部�?|

去重策略：按 `(作用域优先级, 技能名�?` 排序，同名的保留高优先级�?

### 6.3 jeeves 设计

```rust
// backend/src/skills/scope.rs（新建模块）

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum SkillScope {
    /// 项目级技能（仓库�?.jeeves/skills/�?
    Project,
    /// 用户级技能（%LOCALAPPDATA%/jeeves/skills/�?
    User,
    /// 系统级技能（内置，只读）
    System,
}

impl SkillScope {
    /// 优先级数值：越小优先级越�?
    pub fn priority(&self) -> u8 {
        match self {
            SkillScope::Project => 0,
            SkillScope::User => 1,
            SkillScope::System => 2,
        }
    }
}
```

**多目录扫描逻辑�?*

```rust
impl SkillsLoader {
    /// 从多个作用域目录加载技能，按优先级去重
    pub fn load_all_skills(&self) -> Vec<ScopedSkillDef> {
        let mut all_skills = Vec::new();

        // 1. 系统级技能（内置�?
        for skill in self.load_system_skills() {
            all_skills.push(skill.with_scope(SkillScope::System));
        }

        // 2. 用户级技�?
        for skill in self.load_dir(&self.skills_dir) {
            all_skills.push(skill.with_scope(SkillScope::User));
        }

        // 3. 项目级技能（如果存在�?
        if let Some(project_dir) = self.find_project_skills_dir() {
            for skill in self.load_dir(&project_dir) {
                all_skills.push(skill.with_scope(SkillScope::Project));
            }
        }

        // 4. 去重：同名的保留高优先级（Project > User > System�?
        all_skills.sort_by(|a, b| {
            b.scope.priority().cmp(&a.scope.priority())
        });
        all_skills.dedup_by(|a, b| a.name == b.name);

        all_skills
    }
}
```

**目录结构�?*

```
# 系统内置（编译时嵌入二进制或首次运行时释放）
%LOCALAPPDATA%/jeeves/skills/.system/
├── skill-creator/SKILL.md
└── summarizer/SKILL.md

# 用户安装（Phase 1 已有�?
%LOCALAPPDATA%/jeeves/skills/pdf_analysis/SKILL.md

# 项目级（在仓库根目录�?
<project_root>/.jeeves/skills/
└── project-rules/SKILL.md
```

---

## 7. 附录：参考实�?

### 7.1 Anthropic Skills 官方仓库

| 链接 | 用�?|
|------|------|
| [anthropics/skills](https://github.com/anthropics/skills) | 官方仓库，示例技�?|
| [agentskills.io](https://agentskills.io) | Skills 开放标准规�?|

### 7.2 关键文件路径（jeeves 当前�?

| 文件 | 作用 |
|------|------|
| `backend/src/skills/loader.rs` | SkillsLoader + SkillDef |
| `backend/src/tools/builtin.rs` | skills_list + skill_view 工具 |
| `backend/src/agent/prompt.rs` | SystemPromptBuilder（技能索引层�?|
| `backend/src/server/routes/skills.rs` | 技�?HTTP API |
| `docs/agent工具调用优化方案.md` | 工具传递优化讨论记�?|
| `docs/skills/hermes-agent-skills-analysis.md` | Hermes Agent 技能系统分�?|
| `docs/skills/codex-skills-analysis.md` | Codex 技能系统分�?|
| `docs/skills/claw-code-skills-analysis.md` | Claw-Code 技能系统分�?|

### 7.3 实现顺序建议

```
Phase 1 (Skills Hub) �?Phase 2 (Security) �?Phase 3 (Optimization) �?Phase 4 (Scopes)

理由�?
- Phase 1 是功能入口，先打�?Hub 才有第三方技能来�?
- Phase 2 �?Phase 1 的安全保障，必须先做
- Phase 3 �?Phase 4 是体验优化，不阻塞核心功�?
```
