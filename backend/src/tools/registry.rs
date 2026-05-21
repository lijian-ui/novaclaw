use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};

// ── Circuit Breaker 状态常量 ────────────────────────────────────────────────

const CB_CLOSED: u8 = 0;
const CB_OPEN: u8 = 1;
const CB_HALF_OPEN: u8 = 2;

/// 获取当前 Unix 时间戳（秒）
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ── Circuit Breaker ──────────────────────────────────────────────────────────

/// 每个工具独立的熔断器
///
/// 状态机：CLOSED → OPEN → HALF_OPEN → CLOSED
///
/// - **CLOSED**（正常）：工具可执行。连续失败达到 `failure_threshold` 后切换到 OPEN。
/// - **OPEN**（熔断）：直接返回熔断错误，不执行真实 handler。
///    等待 `recovery_timeout_secs` 秒后，下一个请求将自动切换到 HALF_OPEN。
/// - **HALF_OPEN**（探测）：放行一次请求作为探测。
///    成功 → CLOSED（恢复正常）；失败 → OPEN（继续熔断）。
pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU32,
    last_failure_time: AtomicI64,
    consecutive_successes: AtomicU32,
    failure_threshold: u32,
    recovery_timeout_secs: u32,
    recovery_threshold: u32,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("state", &self.state_name())
            .field("failure_count", &self.failure_count.load(Ordering::Relaxed))
            .field("failure_threshold", &self.failure_threshold)
            .field("recovery_timeout_secs", &self.recovery_timeout_secs)
            .field("recovery_threshold", &self.recovery_threshold)
            .finish()
    }
}

impl CircuitBreaker {
    /// 创建熔断器
    ///
    /// - `failure_threshold`：触发熔断的连续失败次数（默认 3）
    /// - `recovery_timeout_secs`：OPEN → HALF_OPEN 的冷却时间（默认 30）
    /// - `recovery_threshold`：HALF_OPEN 下连续探测成功几次后恢复 CLOSED（默认 2）
    pub fn new(failure_threshold: u32, recovery_timeout_secs: u32, recovery_threshold: u32) -> Self {
        Self {
            state: AtomicU8::new(CB_CLOSED),
            failure_count: AtomicU32::new(0),
            last_failure_time: AtomicI64::new(0),
            consecutive_successes: AtomicU32::new(0),
            failure_threshold,
            recovery_timeout_secs,
            recovery_threshold,
        }
    }

    /// 使用默认参数创建熔断器（3次失败 / 30秒冷却 / 2次成功恢复）
    pub fn default_config() -> Self {
        Self::new(3, 30, 2)
    }

    /// 执行前检查熔断器状态
    ///
    /// 返回 `Ok(())` 表示允许执行；返回 `Err(msg)` 表示被熔断拦截。
    pub fn before_call(&self) -> Result<(), String> {
        let state = self.state.load(Ordering::Acquire);
        match state {
            CB_CLOSED => Ok(()),
            CB_OPEN => {
                let now = now_secs();
                let last_fail = self.last_failure_time.load(Ordering::Acquire);
                if now - last_fail >= self.recovery_timeout_secs as i64 {
                    // 冷却时间已到 → 尝试切换到 HALF_OPEN，当前线程成为探测请求
                    if self
                        .state
                        .compare_exchange(CB_OPEN, CB_HALF_OPEN, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        self.consecutive_successes.store(0, Ordering::Release);
                        return Ok(());
                    }
                }
                let remaining = (last_fail + self.recovery_timeout_secs as i64 - now).max(0);
                Err(format!(
                    "工具已被熔断器拦截（OPEN），将在约 {} 秒后自动尝试恢复",
                    remaining
                ))
            }
            CB_HALF_OPEN => {
                Err("工具熔断器正在恢复中（HALF_OPEN），请稍后重试".to_string())
            }
            _ => Ok(()),
        }
    }

    /// 执行后记录结果并更新熔断器状态
    pub fn after_call(&self, success: bool) {
        loop {
            let state = self.state.load(Ordering::Acquire);
            match state {
                CB_CLOSED => {
                    if success {
                        // 成功后重置失败计数
                        self.failure_count.store(0, Ordering::Release);
                    } else {
                        let count = self.failure_count.fetch_add(1, Ordering::AcqRel) + 1;
                        self.last_failure_time.store(now_secs(), Ordering::Release);
                        if count >= self.failure_threshold {
                            let _ = self.state.compare_exchange(
                                CB_CLOSED,
                                CB_OPEN,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            );
                        }
                    }
                    return;
                }
                CB_HALF_OPEN => {
                    if success {
                        let successes = self.consecutive_successes.fetch_add(1, Ordering::AcqRel) + 1;
                        if successes >= self.recovery_threshold {
                            let _ = self.state.compare_exchange(
                                CB_HALF_OPEN,
                                CB_CLOSED,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            );
                            self.failure_count.store(0, Ordering::Release);
                        }
                    } else {
                        let _ = self.state.compare_exchange(
                            CB_HALF_OPEN,
                            CB_OPEN,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        );
                        self.last_failure_time.store(now_secs(), Ordering::Release);
                        self.failure_count.store(0, Ordering::Release);
                    }
                    return;
                }
                CB_OPEN => return, // 防止极少见的并发竞争导致状态回退
                _ => return,
            }
        }
    }

    /// 获取当前状态的文本名称（用于日志/诊断）
    pub fn state_name(&self) -> &'static str {
        match self.state.load(Ordering::Acquire) {
            CB_CLOSED => "CLOSED",
            CB_OPEN => "OPEN",
            CB_HALF_OPEN => "HALF_OPEN",
            _ => "UNKNOWN",
        }
    }

    /// 获取当前熔断器状态的快照（用于外部监控）
    pub fn snapshot(&self) -> CircuitBreakerSnapshot {
        CircuitBreakerSnapshot {
            state: self.state_name().to_string(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            failure_threshold: self.failure_threshold,
            last_failure_time: self.last_failure_time.load(Ordering::Relaxed),
            recovery_timeout_secs: self.recovery_timeout_secs,
            consecutive_successes: self.consecutive_successes.load(Ordering::Relaxed),
            recovery_threshold: self.recovery_threshold,
        }
    }
}

/// 熔断器状态快照（线程安全读取，用于诊断）
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitBreakerSnapshot {
    pub state: String,
    pub failure_count: u32,
    pub failure_threshold: u32,
    pub last_failure_time: i64,
    pub recovery_timeout_secs: u32,
    pub consecutive_successes: u32,
    pub recovery_threshold: u32,
}

// ── 工具定义 & 注册表 ────────────────────────────────────────────────────────

/// 工具定义（含 OpenAI Function Calling Schema）
#[derive(Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub handler: Arc<dyn Fn(Value, Option<mpsc::UnboundedSender<String>>) -> Result<String, String> + Send + Sync>,
}

/// 工具注册表
#[derive(Clone)]
pub struct ToolRegistry {
    pub(crate) tools: Arc<RwLock<HashMap<String, ToolDef>>>,
    /// 熔断器状态表，与 tools 一一对应
    pub(crate) circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry").finish()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl ToolRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册工具（自动附加熔断器）
    pub async fn register(&self, tool: ToolDef) {
        let cb = Arc::new(CircuitBreaker::default_config());

        let mut cbs = self.circuit_breakers.write().await;
        cbs.insert(tool.name.clone(), cb);

        let mut tools = self.tools.write().await;
        tracing::info!("注册工具: {}", tool.name);
        tools.insert(tool.name.clone(), tool);
    }

    /// 获取工具定义
    pub async fn get(&self, name: &str) -> Option<ToolDef> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// 获取所有已注册工具的名称列表（仅名称）
    pub async fn get_all_tool_names(&self) -> Vec<String> {
        let tools = self.tools.read().await;
        let mut names: Vec<String> = tools.keys().cloned().collect();
        names.sort();
        names
    }

    /// 按名称白名单过滤工具，返回一个新的注册表
    /// 用于子 Agent 的受限工具集
    pub async fn filter_by_names(&self, names: &[String]) -> Self {
        let tools = self.tools.read().await;
        let cbs = self.circuit_breakers.read().await;
        let mut filtered_tools = HashMap::new();
        let mut filtered_cbs = HashMap::new();
        for name in names {
            if let Some(tool) = tools.get(name) {
                filtered_tools.insert(name.clone(), tool.clone());
                if let Some(cb) = cbs.get(name) {
                    filtered_cbs.insert(name.clone(), cb.clone());
                }
            }
        }
        ToolRegistry {
            tools: Arc::new(RwLock::new(filtered_tools)),
            circuit_breakers: Arc::new(RwLock::new(filtered_cbs)),
        }
    }

    /// 执行工具（受熔断器 + 超时 + spawn_blocking 保护）
    /// `chunk_tx` 可选：用于流式输出（如 execute_command 的实时终端输出）
    pub async fn execute(&self, name: &str, mut args: Value, workspace: Option<&str>, chunk_tx: Option<mpsc::UnboundedSender<String>>) -> Result<super::types::ToolResult, String> {
        let (handler, circuit_breaker) = {
            let tools = self.tools.read().await;
            let cbs = self.circuit_breakers.read().await;
            match tools.get(name) {
                Some(tool) => (tool.handler.clone(), cbs.get(name).cloned()),
                None => return Err(format!("未知工具: {}", name)),
            }
        };

        // 在参数中注入工作目录（仅当提供时）
        if let Some(ws) = workspace {
            if let Some(obj) = args.as_object_mut() {
                obj.insert("_workspace".to_string(), Value::String(ws.to_string()));
            }
        }

        // 熔断器前置检查
        if let Some(ref cb) = circuit_breaker {
            cb.before_call()?;
        }

        // 在阻塞线程池中执行 handler（spawn_blocking），避免阻塞 tokio worker
        // 同时由 timeout 兜底，防止同步 I/O 工具永久卡死
        let timeout_secs = match name {
            "read_file" | "grep" | "search_replace" => 120u64,
            "write_file" | "edit_file" => 60,
            "glob" | "list_dir" | "rename_file" => 30,
            "web_search" => 60,
            "execute_command" => 300, // 命令执行最多 5 分钟
            _ => 120,
        };
        let timeout = std::time::Duration::from_secs(timeout_secs);

        let h = handler.clone();
        let spawned = tokio::task::spawn_blocking(move || (h)(args, chunk_tx));

        let raw_result = match tokio::time::timeout(timeout, spawned).await {
            Ok(Ok(Ok(output))) => Ok(output),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(join_err)) => Err(format!("Tool execution panicked: {}", join_err)),
            Err(_) => Err(format!("Tool '{}' timed out after {}s", name, timeout_secs)),
        };

        // 将 String 结果转换为 ToolResult
        // 特殊的 JSON 格式 `{"__type":"PendingApproval",...}` 表示需要确认
        let result = match raw_result {
            Ok(s) => {
                if let Ok(val) = serde_json::from_str::<Value>(&s) {
                    if val.get("__type").and_then(|v| v.as_str()) == Some("PendingApproval") {
                        if let Some(approval) = val.get("approval") {
                            if let Ok(apr) = serde_json::from_value::<super::types::ApprovalRequired>(approval.clone()) {
                                Ok(super::types::ToolResult::PendingApproval(apr))
                            } else {
                                Ok(super::types::ToolResult::Success(s))
                            }
                        } else {
                            Ok(super::types::ToolResult::Success(s))
                        }
                    } else {
                        Ok(super::types::ToolResult::Success(s))
                    }
                } else {
                    Ok(super::types::ToolResult::Success(s))
                }
            }
            Err(e) => Err(e),
        };

        // 记录执行结果到熔断器
        if let Some(ref cb) = circuit_breaker {
            let is_success = match &result {
                Ok(super::types::ToolResult::Success(_)) => true,
                Ok(super::types::ToolResult::PendingApproval(_)) => true, // 确认请求不算失败
                Err(_) => false,
            };
            cb.after_call(is_success);
        }

        result
    }

    /// 获取所有工具的 LLM Schema
    pub async fn get_schemas(&self) -> Vec<super::types::ToolDefinition> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|t| super::types::ToolDefinition {
                def_type: "function".to_string(),
                function: super::types::FunctionDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    /// 获取工具数量
    pub async fn count(&self) -> usize {
        let tools = self.tools.read().await;
        tools.len()
    }

    /// 检查工具是否存在
    pub async fn has(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains_key(name)
    }

    /// 获取指定工具的熔断器状态快照（诊断用）
    pub async fn circuit_breaker_state(&self, name: &str) -> Option<CircuitBreakerSnapshot> {
        let cbs = self.circuit_breakers.read().await;
        cbs.get(name).map(|cb| cb.snapshot())
    }

    /// 获取所有工具的熔断器状态（诊断用）
    pub async fn all_circuit_breaker_states(&self) -> Vec<(String, CircuitBreakerSnapshot)> {
        let cbs = self.circuit_breakers.read().await;
        cbs.iter().map(|(name, cb)| (name.clone(), cb.snapshot())).collect()
    }

    /// 移除所有名称以指定前缀开头的工具
    pub async fn remove_by_prefix(&self, prefix: &str) {
        let mut tools = self.tools.write().await;
        let mut cbs = self.circuit_breakers.write().await;
        tools.retain(|k, _| !k.starts_with(prefix));
        cbs.retain(|k, _| !k.starts_with(prefix));
    }

    /// 列出所有名称以指定前缀开头的工具名称
    pub async fn list_names_by_prefix(&self, prefix: &str) -> Vec<String> {
        let tools = self.tools.read().await;
        tools.keys().filter(|k| k.starts_with(prefix)).cloned().collect()
    }

    /// 移除指定名称的工具
    pub async fn remove_by_name(&self, name: &str) {
        let mut tools = self.tools.write().await;
        let mut cbs = self.circuit_breakers.write().await;
        tools.remove(name);
        cbs.remove(name);
    }

    /// 手动重置指定工具的熔断器（恢复 CLOSED 状态）
    pub async fn reset_circuit_breaker(&self, name: &str) -> bool {
        let cbs = self.circuit_breakers.read().await;
        if let Some(cb) = cbs.get(name) {
            cb.state.store(CB_CLOSED, Ordering::Release);
            cb.failure_count.store(0, Ordering::Release);
            cb.consecutive_successes.store(0, Ordering::Release);
            cb.last_failure_time.store(0, Ordering::Release);
            true
        } else {
            false
        }
    }
}
