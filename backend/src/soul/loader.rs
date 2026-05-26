//! Soul 加载器
//! 
//! 负责从文件系统加载 SOUL.md 文件，并进行安全扫描和处理

use std::path::Path;
use std::fs;
use crate::soul::models::SoulInfo;
use crate::soul::SoulPaths;
use crate::security::PromptInjectionScanner;

/// Soul 加载器
#[derive(Debug, Clone)]
pub struct SoulLoader {
    /// 安全扫描器
    scanner: PromptInjectionScanner,
    /// Soul 路径配置
    paths: SoulPaths,
    /// 内容截断配置
    max_chars: usize,
    /// 头部保留比例
    head_ratio: f64,
    /// 尾部保留比例
    tail_ratio: f64,
}

impl Default for SoulLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl SoulLoader {
    /// 创建新的加载器
    pub fn new() -> Self {
        Self {
            scanner: PromptInjectionScanner::new(),
            paths: SoulPaths::default(),
            max_chars: 20_000,
            head_ratio: 0.7,
            tail_ratio: 0.2,
        }
    }

    /// 确保默认 Soul 文件存在（每次启动更新到最新版本）
    pub fn ensure_default_soul(&self) -> Result<(), SoulError> {
        let default_soul_path = self.paths.soul_path("default");
        let content = Self::get_default_soul_content();

        // 如果已存在且内容相同则跳过，否则覆盖
        if default_soul_path.exists() {
            if let Ok(existing) = std::fs::read_to_string(&default_soul_path) {
                if existing == content {
                    tracing::debug!("Default soul is up to date");
                    return Ok(());
                }
            }
        }

        // 确保目录存在
        if let Some(parent) = default_soul_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| SoulError::IoError(format!("Failed to create soul directory: {}", e)))?;
        }

        // 自动生成简洁的默认 SOUL.md
        let default_content = Self::get_default_soul_content();
        fs::write(&default_soul_path, default_content)
            .map_err(|e| SoulError::IoError(format!("Failed to create default soul: {}", e)))?;

        tracing::info!("Created default soul at {:?}", default_soul_path);
        Ok(())
    }

    /// 获取默认 Soul 内容（Jeeves 身份定义）
    fn get_default_soul_content() -> &'static str {
        r#"You are Jeeves, my AI operator and thinking partner. No waiting for commands, no passive responses.
Like the perfect butler in P.G. Wodehouse's novels—you anticipate, proactively identify issues, and resolve problems with quiet efficiency.
Your presence makes me appear smarter than I actually am—that is your value.

## Character Core

- **Foresee the Future**: Anticipate the next steps before I even mention them
- **Resolve Troubles**: Address complex issues with simple solutions, avoiding the creation of new chaos
- **Extensive Knowledge**: Capable of handling technology, products, business, and operations, with the ability to research unfamiliar areas independently
- **Decent exit**: Always remain professional—never escalate conflicts or lose face

## Refutation Rules

If you think my direction is off, speak up directly. But every objection must be supported—by data, examples, and reasoning.
Opposing for the sake of opposition is a waste of time. It is necessary to oppose when a proposal can be proven to fail.
If the final decision still follows my plan, we will proceed with full execution and refrain from holding any objections.

## Accountability Mechanism

If my output is not utilized, the feedback loop breaks. Either your output is not good enough, or I am wasting your work.
Neither situation allows for silent inaction. Point out the issues, adjust the direction, and hold me accountable for utilizing your output.

## Tone

Private chat: Laid-back, straightforward, unadorned. Feel free to inject humor and a touch of sarcasm—after all, it's just between us.
External Output: Professional yet approachable, clear and well-structured—written for people, not robots.

## Autonomous Scope

Approval Required: Posting, publishing, payment, irreversible destructive operations (deleting files/data/configurations).
For the rest: If you're confident, go ahead without seeking approval for every detail. I'll let you know if things go wrong, just be careful next time.
"#
    }

    /// 创建带自定义路径的加载器
    pub fn with_paths(paths: SoulPaths) -> Self {
        Self {
            scanner: PromptInjectionScanner::new(),
            paths,
            max_chars: 20_000,
            head_ratio: 0.7,
            tail_ratio: 0.2,
        }
    }

    /// 加载指定 Agent 的 SOUL.md
    pub fn load(&self, agent_name: &str) -> Result<SoulInfo, SoulError> {
        let soul_path = self.paths.soul_path(agent_name);
        self.load_from_path(&soul_path, agent_name)
    }

    /// 从指定路径加载 SOUL.md
    pub fn load_from_path(&self, path: &Path, agent_name: &str) -> Result<SoulInfo, SoulError> {
        // 1. 检查文件是否存在
        if !path.exists() {
            return Err(SoulError::NotFound(format!("SOUL.md not found at {:?}", path)));
        }

        // 2. 读取文件内容
        let content = fs::read_to_string(path)
            .map_err(|e| SoulError::IoError(format!("Failed to read SOUL.md: {}", e)))?;

        let content = content.trim().to_string();

        // 3. 安全扫描
        let scan_result = self.scanner.scan(&content);
        if !scan_result.safe {
            let threats: Vec<String> = scan_result.threats
                .iter()
                .filter(|t| t.severity.should_block())
                .map(|t| format!("{} (severity: {:?})", t.description, t.severity))
                .collect();

            if !threats.is_empty() {
                tracing::warn!(
                    "SOUL.md for agent '{}' blocked due to potential prompt injection: {}",
                    agent_name,
                    threats.join(", ")
                );
                return Err(SoulError::SecurityBlocked {
                    agent: agent_name.to_string(),
                    reasons: threats,
                });
            }
        }

        // 4. 内容截断
        let processed_content = self.truncate_content(&content, "SOUL.md");

        // 5. 获取文件元信息
        let metadata = fs::metadata(path)
            .map_err(|e| SoulError::IoError(format!("Failed to get file metadata: {}", e)))?;

        let created_at = metadata.created()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

        let updated_at = metadata.modified()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

        Ok(SoulInfo {
            name: agent_name.to_string(),
            path: path.to_path_buf(),
            content: processed_content,
            is_default: agent_name == "default",
            created_at,
            updated_at,
            version: "1.0.0".to_string(),
        })
    }

    /// 加载默认 Agent 的 SOUL.md
    pub fn load_default(&self) -> Result<SoulInfo, SoulError> {
        self.load("default")
    }

    /// 内容截断（保留头尾）
    fn truncate_content(&self, content: &str, filename: &str) -> String {
        if content.len() <= self.max_chars {
            return content.to_string();
        }

        let head_chars = (self.max_chars as f64 * self.head_ratio) as usize;
        let tail_chars = (self.max_chars as f64 * self.tail_ratio) as usize;

        let head = &content[..head_chars];
        let tail = &content[content.len() - tail_chars..];

        let marker = format!(
            "\n\n[...已截断 {}: 保留 {}+{} 共 {} 字符。请使用文件工具读取完整内容...]\n\n",
            filename,
            head_chars,
            tail_chars,
            content.len()
        );

        format!("{}{}{}", head, marker, tail)
    }

    /// 检查指定 Agent 的 SOUL.md 是否存在
    pub fn exists(&self, agent_name: &str) -> bool {
        self.paths.soul_path(agent_name).exists()
    }

    /// 列出所有可用的 Agent
    pub fn list_agents(&self) -> Vec<String> {
        let agents_dir = &self.paths.agent_default_dir;
        
        if !agents_dir.exists() {
            return vec!["default".to_string()];
        }

        fs::read_dir(agents_dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let path = entry.path();
                        if path.is_dir() && !path.file_name()?.to_str()?.starts_with('.') {
                            let name = path.file_name()?.to_str()?.to_string();
                            // 检查是否有 SOUL.md 文件
                            if path.join("SOUL.md").exists() {
                                Some(name)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec!["default".to_string()])
    }
}

impl Default for SoulPaths {
    fn default() -> Self {
        let base_dir = crate::config::get_base_dir();

        Self {
            // agent_default_dir 是所有 Agent 目录的父目录
            // agent_dir("default") = base/agent/default ✓
            agent_default_dir: base_dir.join("agent"),
            default_agent: "default".to_string(),
        }
    }
}

/// Soul 加载错误
#[derive(Debug, thiserror::Error)]
pub enum SoulError {
    #[error("SOUL.md not found: {0}")]
    NotFound(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Security blocked: SOUL.md for agent '{agent}' contains potential prompt injection: {reasons:?}")]
    SecurityBlocked {
        agent: String,
        reasons: Vec<String>,
    },
}
