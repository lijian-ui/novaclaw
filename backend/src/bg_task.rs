//! 后台命令任务管理器
//!
//! 管理长时间运行的 shell 命令，支持提交后台执行和轮询查询结果。
//! 所有操作均为同步，可在工具 handler（spawn_blocking）中直接调用。

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 后台任务状态
#[derive(Debug, Clone, serde::Serialize)]
pub enum BgTaskStatus {
    Running,
    Done,
    Failed(String),
}

/// 后台任务
#[derive(Debug, Clone, serde::Serialize)]
pub struct BgTask {
    pub id: String,
    pub command: String,
    pub workdir: PathBuf,
    pub status: BgTaskStatus,
    pub stdout: String,
    pub exit_code: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

/// 全局后台任务管理器
static BG_TASK_MANAGER: Lazy<Arc<Mutex<HashMap<String, BgTask>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// 获取全局后台任务管理器锁
fn lock() -> std::sync::MutexGuard<'static, HashMap<String, BgTask>> {
    BG_TASK_MANAGER.lock().unwrap()
}

/// 提交一个后台命令，返回 task_id
pub fn submit(command: &str, workdir: PathBuf, timeout_secs: u64) -> String {
    let id = format!(
        "bg_{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("task")
    );
    let now = chrono::Utc::now().to_rfc3339();

    let task = BgTask {
        id: id.clone(),
        command: command.to_string(),
        workdir: workdir.clone(),
        status: BgTaskStatus::Running,
        stdout: String::new(),
        exit_code: None,
        created_at: now.clone(),
        updated_at: now,
    };

    {
        let mut tasks = lock();
        tasks.insert(id.clone(), task);
    }

    // 后台线程执行
    let cmd = command.to_string();
    let wd = workdir.clone();
    let task_id = id.clone();
    std::thread::spawn(move || {
        let result = crate::tools::execute::execute_command_safe(&cmd, &wd, timeout_secs, None, &[], None);

        let now = chrono::Utc::now().to_rfc3339();
        if let Ok(mut tasks) = BG_TASK_MANAGER.lock() {
            if let Some(task) = tasks.get_mut(&task_id) {
                task.stdout = result.stdout;
                task.exit_code = result.exit_code;
                task.updated_at = now;
                if result.blocked {
                    task.status = BgTaskStatus::Failed(format!(
                        "⛔ 命令被安全策略拦截（匹配黑名单模式: {}）。请在设置中移除该关键词后重试。",
                        result.block_reason
                    ));
                } else if result.timed_out {
                    task.status = BgTaskStatus::Failed(format!("Timed out after {}s", timeout_secs));
                } else if result.exit_code == Some(0) {
                    task.status = BgTaskStatus::Done;
                } else {
                    task.status = BgTaskStatus::Failed(format!("Exit code: {:?}", result.exit_code));
                }
            }
        }
    });

    id
}

/// 查询任务状态
pub fn query(task_id: &str) -> Option<BgTask> {
    let tasks = lock();
    tasks.get(task_id).cloned()
}

/// 列出所有运行中的任务
#[allow(dead_code)]
pub fn list_running() -> Vec<BgTask> {
    let tasks = lock();
    tasks
        .values()
        .filter(|t| matches!(t.status, BgTaskStatus::Running))
        .cloned()
        .collect()
}

/// 清理已完成/失败的任务（保留最近 N 条）
#[allow(dead_code)]
pub fn cleanup(keep: usize) {
    let mut tasks = lock();
    let mut entries: Vec<(String, BgTask)> = tasks.drain().collect();
    entries.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
    for (id, task) in entries.into_iter().take(keep) {
        tasks.insert(id, task);
    }
}
