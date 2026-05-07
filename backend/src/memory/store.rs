use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::error::AppError;

/// 记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// 内容
    pub content: String,
    /// 分类
    pub category: String,
    /// 添加时间
    pub added_at: String,
    /// 最后访问时间
    pub last_accessed: String,
    /// 访问次数
    pub access_count: u32,
}

/// 记忆存储
#[derive(Debug, Clone)]
pub struct MemoryStore {
    memory_path: PathBuf,
    user_path: PathBuf,
}

impl MemoryStore {
    /// 创建新的记忆存储
    pub fn new(base_dir: &PathBuf) -> Self {
        fs::create_dir_all(base_dir).ok();

        Self {
            memory_path: base_dir.join("MEMORY.md"),
            user_path: base_dir.join("USER.md"),
        }
    }

    /// 添加记忆
    pub fn add_memory(&self, content: &str, category: &str) -> Result<(), AppError> {
        let memory = MemoryEntry {
            content: content.to_string(),
            category: category.to_string(),
            added_at: chrono::Utc::now().to_rfc3339(),
            last_accessed: chrono::Utc::now().to_rfc3339(),
            access_count: 0,
        };

        let line = serde_json::to_string(&memory).map_err(|e| {
            AppError::Storage(format!("序列化记忆失败: {}", e))
        })?;

        let mut existing = fs::read_to_string(&self.memory_path).unwrap_or_default();
        if !existing.is_empty() {
            existing.push('\n');
        }
        existing.push_str(&line);
        fs::write(&self.memory_path, existing)?;

        Ok(())
    }

    /// 查询记忆
    pub fn query_memories(&self, query: &str) -> Result<Vec<String>, AppError> {
        let content = fs::read_to_string(&self.memory_path).unwrap_or_default();
        let query_lower = query.to_lowercase();

        let mut matches: Vec<String> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| {
                serde_json::from_str::<MemoryEntry>(line).ok()
            })
            .filter(|entry| {
                query.is_empty()
                    || entry.content.to_lowercase().contains(&query_lower)
                    || entry.category.to_lowercase().contains(&query_lower)
            })
            .map(|entry| {
                format!("[{}] {} ({})", entry.category, entry.content, entry.added_at)
            })
            .collect();

        // 更新访问计数
        if !query.is_empty() {
            let _ = self.update_access_counts(query);
        }

        if matches.len() > 10 {
            matches.truncate(10);
            matches.push(format!("... 共 {} 条匹配结果", content.lines().filter(|l| !l.trim().is_empty()).count()));
        }

        Ok(matches)
    }

    /// 删除记忆
    pub fn remove_memory(&self, content_match: &str) -> Result<(), AppError> {
        let content = fs::read_to_string(&self.memory_path).unwrap_or_default();
        let content_lower = content_match.to_lowercase();

        let new_lines: Vec<&str> = content
            .lines()
            .filter(|line| {
                if line.trim().is_empty() {
                    return true;
                }
                if let Ok(entry) = serde_json::from_str::<MemoryEntry>(line) {
                    !entry.content.to_lowercase().contains(&content_lower)
                } else {
                    true
                }
            })
            .collect();

        fs::write(&self.memory_path, new_lines.join("\n"))?;
        Ok(())
    }

    /// 获取所有记忆（格式化）
    pub fn get_all_memories(&self) -> Result<String, AppError> {
        let content = fs::read_to_string(&self.memory_path).unwrap_or_default();
        if content.trim().is_empty() {
            return Ok("暂无记忆".to_string());
        }

        let entries: Vec<String> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| {
                serde_json::from_str::<MemoryEntry>(line).ok()
            })
            .map(|entry| {
                format!("[{}] {} (添加于: {})", entry.category, entry.content, &entry.added_at[..10])
            })
            .collect();

        Ok(entries.join("\n"))
    }

    /// 获取用户档案
    pub fn get_user_profile(&self) -> Result<String, AppError> {
        fs::read_to_string(&self.user_path)
            .map(|s| s.trim().to_string())
            .or(Ok("暂无用户档案".to_string()))
    }

    /// 更新访问计数
    fn update_access_counts(&self, _query: &str) -> Result<(), AppError> {
        // 简化实现：不更新文件中的访问计数（避免频繁写入）
        Ok(())
    }
}
