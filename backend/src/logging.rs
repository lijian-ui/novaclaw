use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tokio::sync::broadcast;
use tracing::Subscriber;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::reload;
use tracing_subscriber::layer::Context;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// 日志条目
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub module: String,
    pub message: String,
    pub task_id: Option<String>,
}

/// 广播通道容量
const BROADCAST_CAPACITY: usize = 4096;

/// 全局广播发送者，用于向所有连接的 WebSocket 客户端推送实时日志
static BROADCASTER: Lazy<broadcast::Sender<LogEntry>> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    tx
});

/// 可重载日志级别过滤器的句柄
static LEVEL_HANDLE: Lazy<Mutex<Option<reload::Handle<EnvFilter, Registry>>>> =
    Lazy::new(|| Mutex::new(None));

/// 获取广播发送者
pub fn get_broadcaster() -> broadcast::Sender<LogEntry> {
    BROADCASTER.clone()
}

fn get_log_dir() -> PathBuf {
    crate::config::get_logs_dir()
}

/// 去掉冗长的 crate 前缀，保留可读模块路径
fn short_module(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("jeeves_backend::") {
        rest.to_string()
    } else {
        target.to_string()
    }
}

/// 格式化当前时间为人类可读格式
fn fmt_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 将 ISO 时间戳字符串转成可读格式 ("2026-05-13T09:33:33Z" → "2026-05-13 17:33:33")
fn fmt_ts_from_iso(iso: &str) -> String {
    // 尝试解析 RFC3339 / ISO 8601
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) {
        return dt.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    // 尝试解析无时区的格式
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S%.f") {
        return dt.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    iso.to_string()
}

/// 动态调整系统日志级别
pub fn set_log_level(level: &str) -> Result<(), String> {
    let handle = LEVEL_HANDLE
        .lock()
        .map_err(|e| format!("获取日志级别锁失败: {}", e))?;
    if let Some(ref handle) = *handle {
        let new_filter = EnvFilter::new(format!("jeeves_backend={}", level));
        handle
            .reload(new_filter)
            .map_err(|e| format!("切换日志级别失败: {}", e))?;
    }
    Ok(())
}

/// 写入任务特定日志（由 agent 在任务执行时调用）
pub fn write_task_log(task_id: &str, level: &str, module: &str, message: &str) {
    let log_dir = get_log_dir().join("tasks");
    fs::create_dir_all(&log_dir).ok();

    let file_path = log_dir.join(format!("{}.log", task_id));
    let timestamp = fmt_timestamp();
    let mod_short = short_module(module);

    let line = format!("[{}] [{}] [{}] {}\n", timestamp, level, mod_short, message);

    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
    {
        let _ = file.write_all(line.as_bytes());
    }

    // 同时广播到 WebSocket 客户端
    let entry = LogEntry {
        timestamp,
        level: level.to_string(),
        module: mod_short,
        message: message.to_string(),
        task_id: Some(task_id.to_string()),
    };
    let _ = BROADCASTER.send(entry);
}

/// 读取任务日志
pub fn read_task_log(task_id: &str) -> Result<Vec<LogEntry>, String> {
    let file_path = get_log_dir().join("tasks").join(format!("{}.log", task_id));
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&file_path).map_err(|e| format!("读取日志失败: {}", e))?;

    let entries = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // 格式: [timestamp] [level] [module] message
            if line.is_empty() || !line.starts_with('[') {
                return None;
            }
            let line = line.strip_prefix('[')?;
            let mut parts = line.splitn(4, "] [");
            let timestamp = parts.next()?.to_string();
            let level = parts.next()?.to_string();
            let module = parts.next()?.to_string();
            let rest = parts.next()?;
            let message = rest.trim_end_matches(']').to_string();

            Some(LogEntry {
                timestamp,
                level,
                module,
                message,
                task_id: Some(task_id.to_string()),
            })
        })
        .collect();

    Ok(entries)
}

/// 删除任务日志文件
pub fn delete_task_log(task_id: &str) -> Result<(), String> {
    let file_path = get_log_dir().join("tasks").join(format!("{}.log", task_id));
    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| format!("删除日志文件失败: {}", e))?;
    }
    Ok(())
}

/// 列出所有有日志文件的任务 ID
pub fn list_task_log_ids() -> Result<Vec<String>, String> {
    let tasks_dir = get_log_dir().join("tasks");
    if !tasks_dir.exists() {
        return Ok(Vec::new());
    }

    let mut ids = Vec::new();
    let read_dir = fs::read_dir(&tasks_dir).map_err(|e| format!("读取日志目录失败: {}", e))?;
    for entry in read_dir.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".log") {
                ids.push(name.trim_end_matches(".log").to_string());
            }
        }
    }
    ids.sort();
    Ok(ids)
}

/// 读取系统日志（来自当天的每日滚动文件）
/// 新文件为 JSON 格式（每行一个 JSON），旧文件兼容文本格式
pub fn read_system_logs(level_filter: Option<&str>) -> Result<Vec<LogEntry>, String> {
    let log_dir = get_log_dir();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let file_path = log_dir.join(format!("system.{}", today));
    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&file_path).map_err(|e| format!("读取系统日志失败: {}", e))?;

    let mut entries: Vec<LogEntry> = content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let line = line.trim();

            // 尝试解析 JSON 格式（新文件）
            if line.starts_with('{') {
                return parse_json_log_line(line);
            }

            // 回退：解析文本格式（旧文件兼容）
            parse_text_log_line(line)
        })
        .collect();

    // 级别过滤
    if let Some(level) = level_filter {
        let upper = level.to_uppercase();
        entries.retain(|e| e.level == upper);
    }

    Ok(entries)
}

/// 解析 JSON 格式日志行
/// {"timestamp":"...","level":"INFO","target":"jeeves_backend::xxx","message":"..."}
fn parse_json_log_line(line: &str) -> Option<LogEntry> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;

    let timestamp_raw = v["timestamp"].as_str()?;
    let level = v["level"].as_str()?;
    // flatten_event(true) 下 message 在顶层，否则在 fields["message"]
    Some(LogEntry {
        timestamp: fmt_ts_from_iso(timestamp_raw),
        level: level.to_string(),
        module: String::new(),
        message: v["message"]
            .as_str()
            .or_else(|| v["fields"]["message"].as_str())?
            .to_string(),
        task_id: None,
    })
}

/// 解析旧版文本格式日志行
/// YYYY-MM-DDTHH:MM:SS.mmmmmmZ  LEVEL target: message
fn parse_text_log_line(line: &str) -> Option<LogEntry> {
    let ts_end = line.find("  ")?;
    let timestamp_raw = &line[..ts_end];
    let after_ts = line[ts_end..].trim_start();

    let level_end = after_ts.find(' ')?;
    let level = after_ts[..level_end].to_string();
    let tail = after_ts[level_end..].trim_start();

    // 尝试按 "target: message" 分割
    if let Some(colon_pos) = tail.find(": ") {
        let potential_target = &tail[..colon_pos];
        // 含 :: 的才认为是模块路径，否则整个是消息
        if potential_target.contains("::") {
            return Some(LogEntry {
                timestamp: fmt_ts_from_iso(timestamp_raw),
                level,
                module: String::new(),
                message: tail[colon_pos + 2..].to_string(),
                task_id: None,
            });
        }
    }

    // 没有 target 的纯文本格式
    Some(LogEntry {
        timestamp: fmt_ts_from_iso(timestamp_raw),
        level,
        module: String::new(),
        message: tail.to_string(),
        task_id: None,
    })
}

// ─── 自定义 Layer：将 tracing 事件广播到 WebSocket ──────────────

struct BroadcastLayer;

impl BroadcastLayer {
    const fn new() -> Self {
        Self
    }
}

/// 用于提取 tracing 事件字段的 Visitor
struct LogVisitor<'a> {
    message: &'a mut String,
}

impl tracing::field::Visit for LogVisitor<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message.push_str(&format!("{:?}", value));
        }
    }
}

impl<S> Layer<S> for BroadcastLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let mut message = String::new();
        let mut visitor = LogVisitor {
            message: &mut message,
        };
        event.record(&mut visitor);

        if message.is_empty() {
            return;
        }

        let entry = LogEntry {
            timestamp: fmt_timestamp(),
            level: metadata.level().to_string(),
            module: String::new(),
            message,
            task_id: None,
        };

        // 忽略发送失败（没有接收者时 channel 关闭）
        let _ = BROADCASTER.send(entry);
    }
}

// ─── 自定义时间格式 ──────────────────────────────────────────────

struct ReadableTimer;

impl tracing_subscriber::fmt::time::FormatTime for ReadableTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

// ─── 初始化 ──────────────────────────────────────────────────────

/// 初始化日志系统：
/// - stdout 终端输出（可读时间，无模块前缀噪音）
/// - 文件 JSON 格式保存（结构化，含完整模块信息，便于解析）
/// - 广播层推送实时日志到 WebSocket
/// - 支持运行时动态切换日志级别
pub fn init() {
    let log_dir = get_log_dir();
    fs::create_dir_all(&log_dir).ok();
    fs::create_dir_all(log_dir.join("tasks")).ok();

    // ── stdout 层（终端输出，隐藏冗长的模块路径） ──
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_timer(ReadableTimer)
        .with_target(false);

    // ── 文件层（每日滚动，JSON 格式保留完整字段） ──
    let json_format = tracing_subscriber::fmt::format()
        .json()
        .with_timer(ReadableTimer)
        .with_target(true)
        .flatten_event(true);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "system");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .event_format(json_format)
        .with_ansi(false);

    // WorkerGuard 必须保持存活，这里 leak 掉防止析构
    Box::leak(Box::new(guard));

    // ── 广播层 ──
    let broadcast_layer = BroadcastLayer::new();

    // ── 可重载的级别过滤器 ──
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("jeeves_backend=info"));
    let (filter_layer, reload_handle) = reload::Layer::new(env_filter);

    // 保存重载句柄
    if let Ok(mut handle) = LEVEL_HANDLE.lock() {
        *handle = Some(reload_handle);
    }

    // ── 组合所有层 ──
    let subscriber = Registry::default()
        .with(filter_layer)
        .with(stdout_layer)
        .with(file_layer)
        .with(broadcast_layer);

    // 使用 try_init 避免在已有全局订阅者时 panic（如 Tauri 桌面端）
    tracing::subscriber::set_global_default(subscriber).ok();
}
