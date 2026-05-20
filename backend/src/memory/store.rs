use crate::error::AppError;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// 记忆存储 — 操作 MEMORY.md（§ 分隔的纯文本格式）
///
/// 例: "User prefers concise responses\n\n§ Project uses Rust 2024 edition\n\n§ Code convention: snake_case"
#[derive(Debug, Clone)]
pub struct MemoryStore {
    memory_path: PathBuf,
    user_path: PathBuf,
}

impl MemoryStore {
    pub fn new(base_dir: &PathBuf) -> Self {
        fs::create_dir_all(base_dir).ok();
        Self {
            memory_path: base_dir.join("MEMORY.md"),
            user_path: base_dir.join("USER.md"),
        }
    }

    // ── 底层读写 ──

    /// 读取所有条目
    fn entries(&self) -> Vec<String> {
        let text = fs::read_to_string(&self.memory_path).unwrap_or_default();
        text.split('§')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 原子写回
    fn write_entries(&self, entries: &[String]) -> Result<(), String> {
        let content = entries.join("\n\n§ ");
        let tmp = self.memory_path.with_extension("md.tmp");
        fs::write(&tmp, &content).map_err(|e| format!("写入失败: {}", e))?;
        fs::rename(&tmp, &self.memory_path).map_err(|e| format!("保存失败: {}", e))?;
        Ok(())
    }

    // ── 公开 API ──

    /// 添加记忆
    pub fn add_memory(&self, content: &str, _category: &str) -> Result<(), AppError> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(AppError::Storage("记忆内容不能为空".to_string()));
        }
        // 去重
        let existing = self.entries();
        let norm = trimmed.to_lowercase();
        if existing.iter().any(|e| e.to_lowercase() == norm) {
            return Err(AppError::Storage(format!("已存在相同记忆: \"{}\"", trimmed)));
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.memory_path)?;
        let existing_text = fs::read_to_string(&self.memory_path).unwrap_or_default();
        if !existing_text.trim().is_empty() {
            write!(file, "\n\n§ ")?;
        }
        write!(file, "{}", trimmed)?;
        file.flush()?;
        Ok(())
    }

    /// 替换记忆
    pub fn replace_memory(&self, old: &str, new: &str) -> Result<String, String> {
        let ot = old.trim();
        let nt = new.trim();
        if ot.is_empty() || nt.is_empty() {
            return Err("旧内容和新内容均不能为空".to_string());
        }
        let entries = self.entries();
        let mut found = false;
        let mut new_entries: Vec<String> = Vec::new();
        for e in &entries {
            if e.to_lowercase() == ot.to_lowercase() {
                new_entries.push(nt.to_string());
                found = true;
            } else {
                new_entries.push(e.clone());
            }
        }
        if !found { return Err(format!("未找到匹配的记忆: \"{}\"", ot)); }
        self.write_entries(&new_entries)?;
        Ok(format!("已更新: \"{}\" → \"{}\"", ot, nt))
    }

    /// 删除记忆
    pub fn remove_memory(&self, content_match: &str) -> Result<(), AppError> {
        let target = content_match.trim().to_lowercase();
        if target.is_empty() {
            return Err(AppError::Storage("请指定要删除的记忆".to_string()));
        }
        let entries = self.entries();
        let mut found = false;
        let new_entries: Vec<String> = entries
            .iter()
            .filter(|e| {
                if e.to_lowercase() == target { found = true; false } else { true }
            })
            .cloned()
            .collect();
        if !found {
            return Err(AppError::Storage(format!("未找到匹配的记忆: \"{}\"", content_match)));
        }
        self.write_entries(&new_entries)
            .map_err(|e| AppError::Storage(e))?;
        Ok(())
    }

    /// 搜索记忆
    /// 先尝试精确匹配，如果没找到则返回全部条目让 LLM 自行判断
    pub fn search_memories(&self, query: &str) -> Vec<String> {
        let all = self.entries();
        if query.is_empty() {
            return all;
        }
        let q = query.to_lowercase();
        let exact: Vec<String> = all
            .iter()
            .filter(|e| e.to_lowercase().contains(&q))
            .cloned()
            .collect();
        if exact.is_empty() && all.len() <= 20 {
            // 没找到精确匹配但记忆不多 → 全部返回让 LLM 自己判断相关性
            all
        } else {
            exact
        }
    }

    /// 查询记忆（兼容旧 API）
    pub fn query_memories(&self, query: &str) -> Result<Vec<String>, AppError> {
        Ok(self.search_memories(query))
    }

    /// 列出所有记忆（给 system prompt 注入用）
    pub fn list_memories(&self) -> String {
        let entries = self.entries();
        if entries.is_empty() {
            return String::new();
        }
        entries.join("\n---\n")
    }

    /// 获取所有记忆（兼容旧 API）
    pub fn get_all_memories(&self) -> Result<String, AppError> {
        let s = self.list_memories();
        Ok(if s.is_empty() { "暂无记忆".to_string() } else { s })
    }

    /// 获取用户档案
    pub fn get_user_profile(&self) -> Result<String, AppError> {
        fs::read_to_string(&self.user_path)
            .map(|s| s.trim().to_string())
            .or(Ok(String::new()))
    }
}
