use crate::agent::session::AgentToolCall;
use std::collections::HashMap;

/// StormBreaker — 滑动窗口重复工具调用检测
///
/// 参考 DeepSeek-Reasonix 的 StormBreaker 实现。
///
/// # 核心逻辑
/// - 维护一个滑动窗口（N 条最近调用记录）
/// - 窗口内同一 (name, args) 出现 ≥ threshold 次 → 压制该调用
/// - **变异调用**（写文件、执行命令等会改变状态的操作）：
///   会清除窗口内所有**只读条目**（read_file 等），
///   这样 read→edit→verify-read 不会误判为重复循环
/// - **风暴豁免**（storm_exempt）：某些只读/无副作用的工具永不触发压制
/// - 每轮用户新输入时重置窗口
pub struct StormBreaker {
    /// 滑动窗口大小
    window_size: usize,
    /// 触发压制的阈值（窗口内同调用出现次数）
    threshold: usize,
    /// 最近调用记录
    recent: Vec<StormEntry>,
    /// 已压制的调用次数（用于自修正逻辑）
    suppressed_count: u32,
    /// 是否已给过自修正机会
    gave_self_correction_chance: bool,
}

#[derive(Debug, Clone)]
struct StormEntry {
    name: String,
    args_fingerprint: String,
    is_mutating: bool,
}

impl StormBreaker {
    /// 创建新的 StormBreaker
    ///
    /// - `window_size`: 滑动窗口大小，默认 6
    /// - `threshold`: 触发压制的重复次数，默认 3
    pub fn new(window_size: usize, threshold: usize) -> Self {
        Self {
            window_size,
            threshold,
            recent: Vec::new(),
            suppressed_count: 0,
            gave_self_correction_chance: false,
        }
    }

    /// 检查一组工具调用，返回哪些应该被压制
    ///
    /// # 返回
    /// - `suppressed`: 被压制的工具调用索引列表
    /// - `notes`: 压制原因说明
    /// - `all_suppressed`: 是否全部被压制
    pub fn inspect_batch(
        &mut self,
        calls: &[AgentToolCall],
    ) -> StormReport {
        let mut report = StormReport {
            suppressed_indices: Vec::new(),
            notes: Vec::new(),
            all_suppressed: false,
        };

        for (idx, call) in calls.iter().enumerate() {
            let is_mutating = is_tool_mutating(&call.name);
            let fingerprint = compute_fingerprint(&call.name, &call.arguments);

            // 如果该工具在上次压制循环中已经给了 stub 响应，计数
            let verdict = self.inspect_single(&call.name, &fingerprint, is_mutating);
            if verdict.suppress {
                report.suppressed_indices.push(idx);
                if let Some(reason) = verdict.reason {
                    report.notes.push(reason);
                }
            }
        }

        report.all_suppressed = report.suppressed_indices.len() == calls.len() && !calls.is_empty();
        report
    }

    /// 检查单个工具调用是否应该被压制
    fn inspect_single(&mut self, name: &str, fingerprint: &str, is_mutating: bool) -> StormVerdict {
        // 风暴豁免工具：从不压制
        if is_storm_exempt(name) {
            return StormVerdict { suppress: false, reason: None };
        }

        if is_mutating {
            // 变异调用：清除窗口内所有只读条目
            // 这样 read→edit→verify-read 不会误判
            self.recent.retain(|e| !e.is_mutating);
        }

        // 统计窗口内相同 (name, args) 的出现次数
        let count = self.recent.iter()
            .filter(|e| e.name == name && e.args_fingerprint == fingerprint)
            .count();

        if count >= self.threshold.saturating_sub(1) {
            return StormVerdict {
                suppress: true,
                reason: Some(format!(
                    "{} called with identical args {} times — repeat-loop guard tripped",
                    name, count + 1
                )),
            };
        }

        // 推入窗口
        self.recent.push(StormEntry {
            name: name.to_string(),
            args_fingerprint: fingerprint.to_string(),
            is_mutating,
        });

        // 窗口滑动
        while self.recent.len() > self.window_size {
            self.recent.remove(0);
        }

        StormVerdict { suppress: false, reason: None }
    }

    /// 获取当前被压制次数
    pub fn suppressed_count(&self) -> u32 {
        self.suppressed_count
    }

    /// 是否已给过自修正机会
    pub fn gave_self_correction(&self) -> bool {
        self.gave_self_correction_chance
    }

    /// 标记已给自修正机会
    pub fn mark_self_correction_given(&mut self) {
        self.gave_self_correction_chance = true;
    }

    /// 重置（每轮用户输入时调用）
    pub fn reset(&mut self) {
        self.recent.clear();
        self.suppressed_count = 0;
        self.gave_self_correction_chance = false;
    }
}

/// StormBreaker 判定结果
struct StormVerdict {
    suppress: bool,
    reason: Option<String>,
}

/// 一次检查的报告
pub struct StormReport {
    pub suppressed_indices: Vec<usize>,
    pub notes: Vec<String>,
    pub all_suppressed: bool,
}

/// 判断工具是否为"变异"操作（会改变状态）
/// 变异调用清除只读记录，防止 read→edit→verify 误判
fn is_tool_mutating(name: &str) -> bool {
    matches!(
        name,
        "write_file"
            | "edit_file"
            | "search_replace"
            | "apply_patch"
            | "rename_file"
            | "delete_file"
            | "execute_command"
            | "execute_code"
    )
}

/// 判断工具是否为"风暴豁免"（永不压制）
/// 通常是只读、无副作用的工具
fn is_storm_exempt(name: &str) -> bool {
    matches!(
        name,
        "read_file"  // 读文件不应被压制——LLM可能需要验证编辑结果
            | "memory"
            | "list_dir"
            | "glob"
            | "grep"
            | "search"
            | "session_search"
            | "web_search"
            | "web_fetch"
            | "todo_list"
            | "skill_view"
    )
}

/// 计算调用指纹（用于比较是否相同）
fn compute_fingerprint(name: &str, args: &str) -> String {
    // 对于 read_file，忽略 range 参数（不同行号读取同一文件视为相同意图）
    if name == "read_file" {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
            if let Some(path) = parsed.get("path").and_then(|v| v.as_str()) {
                // 只基于文件路径生成指纹
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                path.hash(&mut hasher);
                return format!("read_file:{}", hasher.finish());
            }
        }
    }

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    args.hash(&mut hasher);
    format!("{}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_suppression() {
        let mut breaker = StormBreaker::new(6, 3);
        let make_call = |name: &str, args: &str| -> AgentToolCall {
            AgentToolCall {
                id: "test".to_string(),
                name: name.to_string(),
                arguments: args.to_string(),
            }
        };

        // 3 次相同的 read_file → 第 3 次被压制
        let call = make_call("read_file", r#"{"path":"test.txt"}"#);
        assert!(!breaker.inspect_single(&call.name, &compute_fingerprint(&call.name, &call.arguments), false).suppress);
        assert!(!breaker.inspect_single(&call.name, &compute_fingerprint(&call.name, &call.arguments), false).suppress);
        let verdict = breaker.inspect_single(&call.name, &compute_fingerprint(&call.name, &call.arguments), false);
        assert!(verdict.suppress);
    }

    #[test]
    fn test_mutating_clears_read_only() {
        let mut breaker = StormBreaker::new(6, 3);

        // read_file 两次（只读）
        let read = AgentToolCall {
            id: "r".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path":"test.txt"}"#.to_string(),
        };
        let rf = compute_fingerprint(&read.name, &read.arguments);
        breaker.inspect_single(&read.name, &rf, false);
        breaker.inspect_single(&read.name, &rf, false);

        // edit_file（变异）→ 清除只读记录
        let edit = AgentToolCall {
            id: "e".to_string(),
            name: "edit_file".to_string(),
            arguments: r#"{"path":"test.txt"}"#.to_string(),
        };
        let ef = compute_fingerprint(&edit.name, &edit.arguments);
        assert!(!breaker.inspect_single(&edit.name, &ef, true).suppress);

        // 再 read_file → 不应被压制（窗口已清）
        let verdict = breaker.inspect_single(&read.name, &rf, false);
        assert!(!verdict.suppress, "edit 后 read 不应被压制");
    }
}
