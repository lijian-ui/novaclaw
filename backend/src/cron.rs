use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Timelike;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{interval, Duration};

// ─── 类型定义 ────────────────────────────────────────────────

/// 定时任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    /// Cron 表达式或自然语言描述
    pub schedule: String,
    /// 是否启用
    pub enabled: bool,
    /// 执行提示词（LLM 收到后执行的内容）
    pub payload: String,
    /// 创建该任务的会话 ID（执行结果将写回此会话）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub status: String,
    pub run_count: u64,
    pub last_error: Option<String>,
}

// ─── 持久化存储 ──────────────────────────────────────────────

/// CronStore - JSON 文件持久化存储定时任务
pub struct CronStore {
    path: PathBuf,
    jobs: Vec<CronJob>,
}

impl CronStore {
    fn path() -> PathBuf {
        crate::config::get_cron_dir().join("jobs.json")
    }

    /// 从文件加载，不存在则创建空存储
    pub fn load() -> Self {
        let path = Self::path();
        let jobs = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Self { path, jobs }
    }

    /// 保存到文件
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).ok();
        }
        if let Ok(content) = serde_json::to_string_pretty(&self.jobs) {
            let _ = fs::write(&self.path, content);
        }
    }

    // ── CRUD ──

    pub fn list(&self) -> &[CronJob] {
        &self.jobs
    }

    pub fn get(&self, id: &str) -> Option<&CronJob> {
        self.jobs.iter().find(|j| j.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut CronJob> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn add(&mut self, job: CronJob) {
        self.jobs.push(job);
        self.save();
    }

    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut CronJob)) -> bool {
        if let Some(job) = self.get_mut(id) {
            f(job);
            job.updated_at = chrono::Utc::now().to_rfc3339();
            self.save();
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let len = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        let removed = self.jobs.len() < len;
        if removed {
            self.save();
        }
        removed
    }

    /// 获取到期且已启用的任务
    pub fn get_due_jobs(&self) -> Vec<CronJob> {
        let now = chrono::Utc::now();
        self.jobs
            .iter()
            .filter(|j| j.enabled)
            .filter(|j| {
                if let Some(ref next) = j.next_run_at {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(next) {
                        return dt <= now;
                    }
                }
                false
            })
            .cloned()
            .collect()
    }
}

// ─── 调度器 ──────────────────────────────────────────────────

/// 全局调度器状态
static CRON_STORE: Lazy<Arc<Mutex<CronStore>>> = Lazy::new(|| {
    Arc::new(Mutex::new(CronStore::load()))
});

pub fn get_store() -> Arc<Mutex<CronStore>> {
    CRON_STORE.clone()
}

/// 并发控制信号量（最大同时执行 5 个任务）
static CRON_SEMAPHORE: Lazy<Arc<Semaphore>> = Lazy::new(|| {
    Arc::new(Semaphore::new(5))
});

/// 启动后台调度器
pub async fn start_scheduler() {
    tracing::info!("[Cron] 调度器已启动（检查间隔: 60s）");

    tokio::spawn(async {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            if let Err(e) = tick().await {
                tracing::error!("[Cron] 调度器执行出错: {}", e);
            }
        }
    });
}

/// 单次 tick：检查到期任务并执行
async fn tick() -> Result<(), String> {
    let due_jobs_count = {
        let store = CRON_STORE.lock().await;
        store.get_due_jobs().len()
    };
    tracing::info!("[Cron] 检查到期任务... 发现 {} 个待执行", due_jobs_count);

    let due_jobs = {
        let store = CRON_STORE.lock().await;
        store.get_due_jobs()
    };

    for job in due_jobs {
        let permit = CRON_SEMAPHORE.clone().acquire_owned().await;
        let job_id = job.id.clone();

        tokio::spawn(async move {
            let _permit = permit;
            tracing::info!("[Cron] 执行任务: {} ({}) | 调度: {} | 负载: {}",
                job.name, job_id, job.schedule, job.payload);

            // 预先推进下次执行
            let next_run = compute_next_run(&job.schedule);
            CRON_STORE.lock().await.update(&job_id, |j| {
                j.last_run_at = Some(chrono::Utc::now().to_rfc3339());
                j.run_count += 1;
                j.next_run_at = next_run.clone();
                j.status = "running".to_string();
            });

            // 执行任务 payload（通过 Agent 运行）
            let result = execute_cron_job(&job).await;

            match result {
                Ok(output) => {
                    tracing::info!("[Cron] 任务完成: {} | 输出:\n{}", job.name, output);
                    CRON_STORE.lock().await.update(&job_id, |j| {
                        j.status = "idle".to_string();
                    });
                }
                Err(e) => {
                    tracing::error!("[Cron] 任务失败: {} | 错误: {}", job.name, e);
                    CRON_STORE.lock().await.update(&job_id, |j| {
                        j.last_error = Some(e);
                        j.status = "failed".to_string();
                    });
                }
            }
        });
    }

    Ok(())
}

/// 执行 cron 任务
async fn execute_cron_job(job: &CronJob) -> Result<String, String> {
    if job.payload.is_empty() {
        return Ok("[Cron] 无执行内容".to_string());
    }

    // 记录执行到日志
    crate::logging::write_task_log(&job.id, "INFO", "cron", &format!("执行定时任务: {}", job.name));
    crate::logging::write_task_log(&job.id, "INFO", "cron", &format!("提示词: {}", job.payload));

    // 通过 Agent 实际执行 payload
    let state = crate::APP_STATE.read().await;
    let default_model = if state.models_config.default_model.is_empty() {
        String::new()
    } else {
        state.models_config.default_model.clone()
    };

    let provider = state.models_config.default_provider(&default_model)
        .or_else(|| state.models_config.providers.first())
        .cloned();

    let provider = match provider {
        Some(p) => p,
        None => return Err("未找到可用的模型提供商".to_string()),
    };

    let llm_client = crate::llm::client::LlmClient::new(provider, state.config.llm_timeout);
    let tool_registry = Arc::new(state.tool_registry.clone());
    let config = state.config.clone();
    drop(state);

    let skills = Vec::new();
    let agent_session = crate::agent::session::AgentSession::new(
        &format!("cron-{}", job.name),
        &default_model,
        None,
    );

    let mut agent = crate::agent::runtime::AgentRuntime::new(
        agent_session,
        llm_client,
        tool_registry,
        &config,
        skills,
    );

    tracing::info!("[Cron] 开始通过 Agent 执行 payload...");
    let result = agent.run_turn(&job.payload, None, None).await
        .map_err(|e| format!("Agent 执行失败: {}", e))?;

    tracing::info!("[Cron] Agent 执行完成: {} 字符", result.content.len());

    // 如果有 session_id，将执行结果写回会话历史
    if let Some(ref sid) = job.session_id {
        if !result.content.is_empty() {
            let now_str = chrono::Utc::now().to_rfc3339();
            let cron_msg = crate::storage::Message {
                id: format!("cron-{}-{}", job.id, chrono::Utc::now().timestamp()),
                session_id: sid.clone(),
                role: "assistant".to_string(),
                content: format!("⏰ 【{}】第 {} 次执行\n━━━━━━━━━━━━━━━━━━\n\n{}", job.name, job.run_count + 1, result.content),
                created_at: now_str.clone(),
                metadata: Some(format!(r#"{{"cron_job_id":"{}"}}"#, job.id)),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                first_reasoning: None,
                again_reasonings: None,
                reasoning: None,
            };
            let store = crate::APP_STATE.read().await;
            let _ = store.session_store.append_message(sid, &cron_msg);
            drop(store);
            tracing::info!("[Cron] 结果已写入会话: {}", sid);
        }
    }

    Ok(result.content)
}

/// 计算下一次执行时间
pub fn compute_next_run(schedule: &str) -> Option<String> {
    let now = chrono::Utc::now();

    // 检查是不是 ISO 时间戳（一次性任务）
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(schedule) {
        if dt > now {
            return Some(dt.to_rfc3339());
        }
        return None; // 已过期
    }

    // 检查是不是间隔描述 like "30m", "2h"
    if let Some(next) = parse_duration_next(schedule, &now) {
        return Some(next);
    }

    // 检查是否是标准 cron 表达式
    if is_valid_cron(schedule) {
        // 简单近似：向前推 1 分钟到 24 小时
        return compute_cron_next(schedule, &now);
    }

    // 检查是否是自然语言调度描述（如 "daily at 2:30 PM"）
    if let Some(next) = parse_natural_schedule(schedule, &now) {
        return Some(next);
    }

    None
}

/// 解析自然语言调度描述，如 "daily at 2:30 PM"、"每天下午14点30"、"hourly" 等
fn parse_natural_schedule(schedule: &str, now: &chrono::DateTime<chrono::Utc>) -> Option<String> {
    let s = schedule.trim().to_lowercase();

    // "hourly" / "every hour"
    if s == "hourly" || s == "every hour" || s == "每小时" || s.starts_with("every 1 hour") {
        return Some((*now + chrono::Duration::hours(1)).to_rfc3339());
    }

    // "daily" / "every day" - 默认推到次日凌晨0点
    if s == "daily" || s == "every day" || s.starts_with("daily") || s == "每天" || s.starts_with("everyday") {
        // 尝试提取具体时间 "daily at H:MM am/pm"
        let re = regex::Regex::new(
            r"(?i)(?:at|在|于|@)?\s*(\d{1,2}):(\d{2})\s*(am|pm|上午|下午|a\.m\.|p\.m\.)?"
        ).ok()?;

        if let Some(caps) = re.captures(schedule) {
            let hour: u32 = caps[1].parse().ok()?;
            let minute: u32 = caps[2].parse().ok()?;
            let meridian = caps.get(3).map(|m| m.as_str().to_lowercase());

            // 12小时制 → 24小时制
            let hour24 = match meridian.as_deref() {
                Some("pm") | Some("p.m.") | Some("下午") => {
                    if hour == 12 { 12 } else { hour + 12 }
                }
                Some("am") | Some("a.m.") | Some("上午") => {
                    if hour == 12 { 0 } else { hour }
                }
                _ => hour, // 已经是24小时制或没有标识
            };

            // 构建今天的该时间
            let target = now.date_naive().and_hms_opt(hour24, minute, 0)?;
            let target_dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(target, chrono::Utc);

            if target_dt > *now {
                return Some(target_dt.to_rfc3339());
            } else {
                // 今天已过，推到明天
                let tomorrow = target_dt + chrono::Duration::days(1);
                return Some(tomorrow.to_rfc3339());
            }
        } else {
            // 没有具体时间，默认每天凌晨0点
            let tomorrow_midnight = (*now + chrono::Duration::days(1))
                .date_naive()
                .and_hms_opt(0, 0, 0)?;
            return Some(
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(tomorrow_midnight, chrono::Utc)
                    .to_rfc3339()
            );
        }
    }

    // 中文自然语言："每天下午14点30"、"每天早上8点"
    {
        let re = regex::Regex::new(
            r"(?i)每天(?:上午|下午|早上|晚上|凌晨)?\s*(\d{1,2})[：:点\.](\d{1,2})?分?"
        ).ok()?;
        if let Some(caps) = re.captures(schedule) {
            let hour: u32 = caps[1].parse().ok()?;
            let minute: u32 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);

            // 检测是否包含"下午"、"晚上"等
            let is_pm = schedule.contains("下午") || schedule.contains("晚上");
            let hour24 = if is_pm && hour < 12 { hour + 12 } else { hour };

            let target = now.date_naive().and_hms_opt(hour24, minute, 0)?;
            let target_dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(target, chrono::Utc);

            if target_dt > *now {
                return Some(target_dt.to_rfc3339());
            } else {
                let tomorrow = target_dt + chrono::Duration::days(1);
                return Some(tomorrow.to_rfc3339());
            }
        }
    }

    None
}

fn parse_duration_next(schedule: &str, now: &chrono::DateTime<chrono::Utc>) -> Option<String> {
    let s = schedule.trim();
    if s.ends_with('m') {
        let num: u64 = s.trim_end_matches('m').parse().ok()?;
        return Some((*now + chrono::Duration::minutes(num as i64)).to_rfc3339());
    }
    if s.ends_with('h') {
        let num: u64 = s.trim_end_matches('h').parse().ok()?;
        return Some((*now + chrono::Duration::hours(num as i64)).to_rfc3339());
    }
    if s.ends_with('d') {
        let num: u64 = s.trim_end_matches('d').parse().ok()?;
        return Some((*now + chrono::Duration::days(num as i64)).to_rfc3339());
    }
    None
}

fn compute_cron_next(expr: &str, now: &chrono::DateTime<chrono::Utc>) -> Option<String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }

    let minute = parse_cron_field(parts[0], 0, 59);
    let hour = parse_cron_field(parts[1], 0, 23);
    let _day_of_month = parse_cron_field(parts[2], 1, 31);
    let _month = parse_cron_field(parts[3], 1, 12);
    let _day_of_week = parse_cron_field(parts[4], 0, 7);

    let minute = minute.unwrap_or(-1);
    let hour = hour.unwrap_or(-1);

    // 简单实现：如果是固定时间点（如 "0 8 * * *"），推到下一个该时间
    if minute >= 0 && hour >= 0 {
        let mut candidate = *now + chrono::Duration::minutes(1);
        // 最多向前找 48 小时
        for _ in 0..2880 {
            if candidate.minute() == minute as u32 && candidate.hour() as i32 == hour {
                return Some(candidate.to_rfc3339());
            }
            candidate = candidate + chrono::Duration::minutes(1);
        }
    }

    // 兜底：1 小时后
    Some((*now + chrono::Duration::hours(1)).to_rfc3339())
}

fn parse_cron_field(field: &str, min: u32, max: u32) -> Option<i32> {
    if field == "*" || field.contains('/') || field.contains(',') || field.contains('-') {
        return None; // 复杂表达式，简化处理
    }
    let val: u32 = field.parse().ok()?;
    if val >= min && val <= max {
        Some(val as i32)
    } else {
        None
    }
}

/// 为刚创建的任务计算第一次执行时间
pub fn compute_initial_next_run(schedule: &str) -> String {
    let now = chrono::Utc::now();

    // 检查是否 ISO 时间戳
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(schedule) {
        if dt > now {
            return dt.to_rfc3339();
        }
    }

    // 检查间隔描述
    if let Some(next) = parse_duration_next(schedule, &now) {
        return next;
    }

    // 如果是定时（每天早上X点），计算下一次
    if is_valid_cron(schedule) {
        if let Some(next) = compute_cron_next(schedule, &now) {
            return next;
        }
    }

    // 自然语言调度描述
    if let Some(next) = parse_natural_schedule(schedule, &now) {
        return next;
    }

    // 兜底：1小时后
    (now + chrono::Duration::hours(1)).to_rfc3339()
}

/// 检查字符串是否为有效的 5 字段 cron 表达式
fn is_valid_cron(expr: &str) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    parts.iter().all(|p| {
        p == &"*" || p.chars().all(|c| c.is_ascii_digit() || "*,/-".contains(c))
    })
}
