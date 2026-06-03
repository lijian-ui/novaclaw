use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 技能定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    /// 名称
    pub name: String,
    /// 描述
    pub description: String,
    /// 版本
    pub version: String,
    /// 内容（从 SKILL.md 加载的完整指令）
    pub content: String,
    /// 来源路径
    pub source_path: String,
    /// 是否启用
    pub enabled: bool,
}

/// Skills 加载器
#[derive(Debug, Clone)]
pub struct SkillsLoader {
    skills_dir: PathBuf,
}

impl SkillsLoader {
    /// 创建新的技能加载器
    pub fn new(skills_dir: &PathBuf) -> Self {
        std::fs::create_dir_all(skills_dir).ok();
        Self {
            skills_dir: skills_dir.to_path_buf(),
        }
    }

    /// 列出所有已安装的技能
    pub fn list_skills(&self) -> Vec<SkillDef> {
        let mut skills = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }

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

    /// 获取指定名称的技能
    pub fn get_skill(&self, name: &str) -> Option<SkillDef> {
        self.list_skills().into_iter().find(|s| s.name == name)
    }

    /// 搜索技能
    pub fn search_skills(&self, query: &str) -> Vec<SkillDef> {
        let query_lower = query.to_lowercase();
        self.list_skills()
            .into_iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// 删除技能
    pub fn delete_skill(&self, name: &str) -> Result<(), String> {
        let skill_dir = self.skills_dir.join(name);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)
                .map_err(|e| format!("删除技能目录失败: {}", e))?;
        }
        Ok(())
    }

    /// 根据全局 config 的 skills map 覆盖 enabled 状态
    pub fn apply_enabled_states(skills: &mut [SkillDef], enabled_map: &std::collections::HashMap<String, bool>) {
        for skill in skills.iter_mut() {
            if let Some(&enabled) = enabled_map.get(&skill.name) {
                skill.enabled = enabled;
            }
        }
    }

    /// 过滤出已启用的技能（基于全局 config 的 skills map）
    pub fn filter_enabled(skills: Vec<SkillDef>, enabled_map: &std::collections::HashMap<String, bool>) -> Vec<SkillDef> {
        let mut result = skills;
        Self::apply_enabled_states(&mut result, enabled_map);
        result.into_iter().filter(|s| s.enabled).collect()
    }

    /// 安装/创建新技能
    pub fn install_skill(&self, skill: &SkillDef) -> Result<(), String> {
        let skill_dir = self.skills_dir.join(&skill.name);
        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("创建技能目录失败: {}", e))?;

        let content = format!(
            "---\nname: {}\ndescription: {}\nversion: {}\n---\n\n{}",
            skill.name, skill.description, skill.version, skill.content
        );

        let skill_md = skill_dir.join("SKILL.md");
        std::fs::write(&skill_md, content)
            .map_err(|e| format!("写入技能文件失败: {}", e))?;

        Ok(())
    }

    /// 解析 SKILL.md 文件内容
    fn parse_skill_md(content: &str, dir_path: &PathBuf) -> Option<SkillDef> {
        Self::parse_skill_md_raw(content).map(|mut s| {
            s.source_path = dir_path.display().to_string();
            s
        })
    }

    /// 解析 SKILL.md 内容（纯解析，不绑定路径），公开给外部使用
    pub fn parse_skill_md_raw(content: &str) -> Option<SkillDef> {
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

        if name.is_empty() {
            return None;
        }

        let body_content: String = content
            .lines()
            .skip(body_start)
            .collect::<Vec<&str>>()
            .join("\n");

        Some(SkillDef {
            name,
            description: if description.is_empty() {
                "无描述".to_string()
            } else {
                description
            },
            version,
            content: body_content,
            source_path: String::new(), // 无路径信息
            enabled: true,
        })
    }
}
