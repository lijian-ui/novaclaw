use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::time::{Duration, Instant};

/// 内置命令白名单（安全只读命令，跳过审批直接执行）
pub const BUILTIN_ALLOWLIST: &[&str] = &[
    // ─── 文件查看 ───
    "ls", "dir", "cat", "head", "tail", "less", "more", "type",
    "find", "grep", "findstr", "select-string", "where", "where-object",
    "wc", "sort", "uniq", "tree",
    // ─── Git 只读 ───
    "git status", "git log", "git diff", "git branch", "git show",
    "git stash list",
    // ─── 目录操作 ───
    "pwd", "cd", "echo", "which", "get-location", "get-childitem",
    "get-item", "test-path",
    // ─── 信息查询 ───
    "npm ls", "npm list", "npm view", "cargo check", "cargo metadata",
    "go list", "python3 --version", "node --version", "npm --version",
    "rustc --version", "cargo --version",
    // ─── 网络诊断 ───
    "ping", "curl -s", "curl --head", "wget --spider", "netstat", "ss",
    "ipconfig", "ifconfig", "get-netipaddress",
    // ─── 系统信息 ───
    "date", "hostname", "uname", "whoami", "ps", "tasklist",
    "df", "du", "free", "top -n", "uptime", "get-process",
    "get-service", "get-date",
];

/// 可运行时更新的白名单缓存（项目级，来自配置文件的 allowlist）
pub static ALLOW_PATTERNS: once_cell::sync::Lazy<RwLock<Vec<String>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(Vec::new()));

/// 可运行时更新的黑名单缓存（pub 以便 config.rs 热更新）
pub static DENY_PATTERNS: once_cell::sync::Lazy<RwLock<Vec<String>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(Vec::new()));

/// 加载项目级白名单到缓存
pub fn load_allow_patterns() -> Vec<String> {
    let patterns = match crate::APP_STATE.try_read() {
        Ok(state) => state.config.shell_allowlist.clone(),
        Err(_) => {
            tracing::warn!("[ExecTool] APP_STATE try_read 失败，使用缓存的白名单");
            if let Ok(cached) = ALLOW_PATTERNS.read() {
                if !cached.is_empty() {
                    return cached.clone();
                }
            }
            Vec::new()
        }
    };
    if let Ok(mut cached) = ALLOW_PATTERNS.write() {
        *cached = patterns.clone();
    }
    patterns
}

/// 检查命令是否命中白名单（内置 + 项目级）
pub fn check_command_allow(command: &str) -> bool {
    // 先确保项目级白名单缓存是最新的（与 APP_STATE.config 同步）
    let _ = load_allow_patterns();
    let cmd_lower = command.to_lowercase().trim().to_string();
    // 1. 检查内置白名单
    for &pattern in BUILTIN_ALLOWLIST {
        let pat_lower = pattern.to_lowercase();
        if cmd_lower.starts_with(&pat_lower) {
            tracing::debug!("[ExecTool] 命令 '{}' 匹配内置白名单 '{}'，直行", command, pattern);
            return true;
        }
    }
    // 2. 检查项目级白名单
    if let Ok(allowed) = ALLOW_PATTERNS.read() {
        for pattern in allowed.iter() {
            let pat_lower = pattern.to_lowercase();
            if cmd_lower.starts_with(&pat_lower) {
                tracing::debug!("[ExecTool] 命令 '{}' 匹配项目白名单 '{}'，直行", command, pattern);
                return true;
            }
        }
    }
    false
}

/// 从配置文件加载黑名单到缓存。如果配置中没有设置，则清空缓存（不阻止任何命令）
pub fn load_deny_patterns() -> Vec<String> {
    let patterns = match crate::APP_STATE.try_read() {
        Ok(state) => state.config.deny_patterns.clone(),
        Err(_) => {
            tracing::warn!("[ExecTool] APP_STATE try_read 失败（锁竞争），使用缓存的拒绝模式");
            if let Ok(cached) = DENY_PATTERNS.read() {
                if !cached.is_empty() {
                    tracing::info!("[ExecTool] 从缓存加载 {} 条拒绝模式", cached.len());
                    return cached.clone();
                }
            }
            Vec::new()
        }
    };
    if let Ok(mut cached) = DENY_PATTERNS.write() {
        *cached = patterns.clone();
    }
    if !patterns.is_empty() {
        tracing::info!("[ExecTool] 已加载 {} 条拒绝模式: {:?}", patterns.len(), patterns);
    }
    patterns
}

/// 命令执行结果
pub struct CommandOutput {
    pub stdout: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
    pub blocked: bool,
    pub block_reason: String,
}



/// 检查命令是否被黑名单拦截（词边界匹配 + 子串匹配，大小写不敏感）
pub fn check_command_deny<'a>(command: &str, patterns: &'a [String]) -> Option<&'a str> {
    if patterns.is_empty() {
        tracing::debug!("[ExecTool] 拒绝模式列表为空，不拦截命令: '{}'", command);
        return None;
    }
    let cmd_lower = command.to_lowercase();
    for pattern in patterns {
        let pat_lower = pattern.to_lowercase();
        // 对多词模式（含空格）使用精确子串匹配
        if pat_lower.contains(' ') {
            if cmd_lower.contains(&pat_lower) {
                tracing::warn!(
                    "[ExecTool] 命令 '{}' 匹配拒绝模式 '{}'，已拦截",
                    command, pattern
                );
                return Some(pattern);
            }
        } else {
            // 对单词模式使用词边界匹配，避免误匹配（如 "del" 误匹配 "model", "ssh" 误匹配 "cross-build"）
            let mut search_start = 0;
            while let Some(pos) = cmd_lower[search_start..].find(&pat_lower) {
                let abs_pos = search_start + pos;
                // 检查前边界：字符串开头 或 前一个字符不是字母数字
                let prev_is_boundary = abs_pos == 0
                    || !cmd_lower.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
                // 检查后边界：字符串结尾 或 后一个字符不是字母数字
                let after_end = abs_pos + pat_lower.len();
                let next_is_boundary = after_end >= cmd_lower.len()
                    || !cmd_lower.as_bytes()[after_end].is_ascii_alphanumeric();
                if prev_is_boundary && next_is_boundary {
                    tracing::warn!(
                        "[ExecTool] 命令 '{}' 匹配拒绝模式 '{}'，已拦截",
                        command, pattern
                    );
                    return Some(pattern);
                }
                search_start = abs_pos + 1;
            }
        }
    }
    tracing::debug!("[ExecTool] 命令 '{}' 未匹配任何拒绝模式", command);
    None
}

/// 构建 shell 命令（跨平台）
fn build_shell_command(command: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "windows") {
        let shell = "powershell.exe".to_string();
        let args = vec![
            "-NoLogo".to_string(),
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-Command".to_string(),
            command.to_string(),
        ];
        (shell, args)
    } else {
        let shell = "bash".to_string();
        let args = vec!["-c".to_string(), command.to_string()];
        (shell, args)
    }
}

/// 在 PTY 中同步执行命令，返回完整输出
///
/// 在当前线程中阻塞执行，适合在 tokio::task::spawn_blocking 或
/// std::thread::spawn 中使用。
pub fn execute_sync(
    command: &str,
    workdir: &std::path::Path,
    timeout_secs: u64,
    chunk_callback: Option<Box<dyn Fn(String) + Send>>,
    deny_patterns: &[String],
    cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> CommandOutput {
    tracing::info!(
        "[ExecTool] Starting command: '{}' | workdir: {} | timeout: {}s",
        command,
        workdir.display(),
        timeout_secs,
    );

    // 命令安全检查
    if let Some(pat) = check_command_deny(command, deny_patterns) {
        tracing::warn!("[ExecTool] Command BLOCKED by deny pattern: {}", pat);
        return CommandOutput {
            stdout: String::new(),
            exit_code: None,
            timed_out: false,
            truncated: false,
            blocked: true,
            block_reason: format!("Command blocked by security policy (matched pattern: {})", pat),
        };
    }
    tracing::debug!("[ExecTool] Security check passed");

    let (shell, args) = build_shell_command(command);
    tracing::debug!("[ExecTool] Shell: {} | Args: {:?}", shell, args);

    // 使用 std::process::Command 执行命令（避免 portable-pty 在 Windows 上弹出可见控制台）
    let mut cmd = Command::new(&shell);
    cmd.args(&args)
        .current_dir(workdir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Windows 上隐藏控制台窗口
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = match cmd.spawn() {
        Ok(c) => {
            tracing::debug!("[ExecTool] Command spawned");
            c
        }
        Err(e) => {
            tracing::error!("[ExecTool] Failed to spawn command: {}", e);
            return CommandOutput {
                stdout: format!("Failed to spawn command: {}", e),
                exit_code: None,
                timed_out: false,
                truncated: false,
                blocked: false,
                block_reason: String::new(),
            };
        }
    };

    let kill_flag = Arc::new(AtomicBool::new(false));
    let kill_flag_clone = kill_flag.clone();
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    // 后台线程：读取 stdout，同时通过回调推送实时块
    tracing::debug!("[ExecTool] Starting reader thread...");
    let chunk_cb = chunk_callback;
    let reader_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        if let Some(mut reader) = stdout_pipe {
            let mut buf = [0u8; 8192];
            loop {
                if kill_flag_clone.load(Ordering::Relaxed) {
                    tracing::trace!("[ExecTool] Reader thread: kill flag set, stopping");
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => {
                        tracing::trace!("[ExecTool] Reader thread: EOF");
                        break;
                    }
                    Ok(n) => {
                        output.extend_from_slice(&buf[..n]);
                        if let Some(ref cb) = chunk_cb {
                            let cleaned = strip_ansi(&buf[..n]);
                            if !cleaned.is_empty() {
                                if let Ok(text) = String::from_utf8(cleaned) {
                                    cb(text);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::trace!("[ExecTool] Reader thread read error: {}", e);
                        break;
                    }
                }
            }
        }
        tracing::debug!("[ExecTool] Reader thread finished, total: {} bytes", output.len());
        output
    });

    // 后台线程：读取 stderr 并合并
    let kill_flag_stderr = kill_flag.clone();
    let stderr_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        if let Some(mut reader) = stderr_pipe {
            let mut buf = [0u8; 8192];
            loop {
                if kill_flag_stderr.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => output.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
        }
        output
    });

    // 等待完成或超时
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut timed_out = false;
    let mut last_warn_time = Instant::now();

    loop {
        if Instant::now() >= deadline {
            tracing::warn!("[ExecTool] Timeout reached ({}s elapsed)", timeout_secs);
            timed_out = true;
            break;
        }

        // 检查用户取消信号
        if let Some(ref flag) = cancel_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!("[ExecTool] Cancel requested by user");
                timed_out = true; // 复用 timed_out 标志表示被取消
                break;
            }
        }

        if last_warn_time.elapsed() > Duration::from_secs(5) {
            let elapsed = deadline.saturating_duration_since(Instant::now()).as_secs();
            tracing::debug!("[ExecTool] Waiting for command... timeout remaining: ~{}s", elapsed);
            last_warn_time = Instant::now();
        }

        if let Ok(Some(_status)) = child.try_wait() {
            tracing::debug!("[ExecTool] Process exited");
            break;
        }

        std::thread::sleep(Duration::from_secs(1));
    }

    // 超时后杀掉进程
    if timed_out {
        tracing::warn!("[ExecTool] Killing command due to timeout");
        kill_flag.store(true, Ordering::Relaxed);
        let _ = child.kill();
    }
    let _ = child.wait();

    // 等待读取线程完成
    tracing::debug!("[ExecTool] Joining reader thread...");
    let mut raw_output = match reader_thread.join() {
        Ok(out) => {
            tracing::debug!("[ExecTool] Reader thread joined, output size: {} bytes", out.len());
            out
        }
        Err(_) => {
            tracing::error!("[ExecTool] Reader thread panicked!");
            Vec::new()
        }
    };
    // 合并 stderr 输出
    if let Ok(stderr_output) = stderr_thread.join() {
        if !stderr_output.is_empty() {
            if !raw_output.is_empty() {
                raw_output.push(b'\n');
            }
            raw_output.extend_from_slice(&stderr_output);
        }
    }

    // 获取退出码
    let exit_code = child.try_wait().ok().flatten().map(|s| s.success() as i32);
    tracing::info!(
        "[ExecTool] Command completed | exit_code: {:?} | timed_out: {} | output_size: {}",
        exit_code, timed_out, raw_output.len(),
    );

    // 清洗 ANSI 转义序列（无需外部依赖）
    tracing::debug!("[ExecTool] Stripping ANSI escape sequences...");
    let cleaned = strip_ansi(&raw_output);
    let mut text = String::from_utf8_lossy(&cleaned).to_string();
    tracing::debug!("[ExecTool] ANSI stripped, text length: {}", text.len());

    // 截断大输出
    const MAX_OUTPUT_LEN: usize = 10_000;
    let truncated = text.len() > MAX_OUTPUT_LEN;
    if truncated {
        tracing::info!("[ExecTool] Output truncated from {} to {} chars", text.len(), MAX_OUTPUT_LEN);
        text.truncate(MAX_OUTPUT_LEN);
        text.push_str("\n\n[Output truncated at 10KB]");
    }

    CommandOutput {
        stdout: text,
        exit_code,
        timed_out,
        truncated,
        blocked: false,
        block_reason: String::new(),
    }
}

/// 安全地执行 shell 命令（线程安全包装，供工具 handler 调用）
///
/// 在独立线程中执行命令，避免阻塞 tokio worker 线程。
pub fn execute_command_safe(
    command: &str,
    workdir: &std::path::Path,
    timeout_secs: u64,
    chunk_callback: Option<Box<dyn Fn(String) + Send>>,
    deny_patterns: &[String],
    cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> CommandOutput {
    let command = command.to_string();
    let workdir = workdir.to_path_buf();
    let timeout_secs = timeout_secs.min(300);
    // 如果未传入模式，从缓存/配置加载
    let patterns: Vec<String> = if deny_patterns.is_empty() {
        let loaded = load_deny_patterns();
        tracing::debug!("[ExecTool] 从配置加载 {} 条拒绝模式用于命令 '{}'", loaded.len(), command);
        loaded
    } else {
        tracing::debug!("[ExecTool] 使用传入的 {} 条拒绝模式用于命令 '{}'", deny_patterns.len(), command);
        deny_patterns.to_vec()
    };

    tracing::info!(
        "[ExecTool] execute_command_safe: '{}' | workdir: {} | timeout: {}s | deny_patterns: {}",
        command,
        workdir.display(),
        timeout_secs,
        patterns.len(),
    );

    std::thread::spawn(move || {
        let result = execute_sync(&command, &workdir, timeout_secs, chunk_callback, &patterns, cancel_flag);
        if result.blocked {
            tracing::warn!("[ExecTool] Result: BLOCKED - {}", result.block_reason);
        } else if result.timed_out {
            tracing::warn!("[ExecTool] Result: TIMEOUT after {}s", timeout_secs);
        } else {
            tracing::info!("[ExecTool] Result: OK | exit_code: {:?}", result.exit_code);
        }
        result
    })
    .join()
    .unwrap_or_else(|_| {
        tracing::error!("[ExecTool] Execution thread panicked!");
        CommandOutput {
            stdout: "Command execution thread crashed".to_string(),
            exit_code: None,
            timed_out: false,
            truncated: false,
            blocked: false,
            block_reason: String::new(),
        }
        })
}

/// 简易 ANSI 转义序列清洗器，无需外部依赖
fn strip_ansi(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x1b {
            // ESC 序列开始
            i += 1;
            if i < input.len() && input[i] == b'[' {
                // CSI 序列: ESC[... 直到遇到字母结束
                i += 1;
                while i < input.len() {
                    let b = input[i];
                    if (0x40..=0x7e).contains(&b) {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            // 其他 ESC 序列（如 ESC]... 操作系统命令）
            while i < input.len() {
                let b = input[i];
                if b == 0x07 || (0x40..=0x7e).contains(&b) {
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else {
            // 普通字符
            output.push(input[i]);
            i += 1;
        }
    }
    output
}
