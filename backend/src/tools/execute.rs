use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// 可运行时更新的黑名单缓存（pub 以便 config.rs 热更新）
pub static DENY_PATTERNS: once_cell::sync::Lazy<RwLock<Vec<String>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(Vec::new()));

/// 从配置文件加载黑名单到缓存。如果配置中没有设置，则清空缓存（不阻止任何命令）
pub fn load_deny_patterns() -> Vec<String> {
    let patterns = match crate::APP_STATE.try_read() {
        Ok(state) => state.config.deny_patterns.clone(),
        Err(_) => Vec::new(),
    };
    if let Ok(mut cached) = DENY_PATTERNS.write() {
        *cached = patterns.clone();
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



/// 检查命令是否被黑名单拦截（子串匹配，大小写不敏感）
fn check_command_deny<'a>(command: &str, patterns: &'a [String]) -> Option<&'a str> {
    let cmd_lower = command.to_lowercase();
    for pattern in patterns {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return Some(pattern);
        }
    }
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

    // 准备 PTY
    tracing::debug!("[ExecTool] Creating PTY...");
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 200,
        cols: 500,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => {
            tracing::debug!("[ExecTool] PTY created successfully");
            p
        }
        Err(e) => {
            tracing::error!("[ExecTool] Failed to create PTY: {}", e);
            return CommandOutput {
                stdout: format!("Failed to create PTY: {}", e),
                exit_code: None,
                timed_out: false,
                truncated: false,
                blocked: false,
                block_reason: String::new(),
            };
        }
    };

    // 构建并启动命令
    let mut cmd_builder = CommandBuilder::new(&shell);
    cmd_builder.cwd(workdir);
    for arg in &args {
        cmd_builder.arg(arg);
    }

    tracing::debug!("[ExecTool] Spawning command in PTY...");
    let mut child = match pair.slave.spawn_command(cmd_builder) {
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

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[ExecTool] Failed to get PTY reader: {}", e);
            let _ = child.kill();
            return CommandOutput {
                stdout: format!("Failed to get PTY reader: {}", e),
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

    // 后台线程：读取所有 PTY 输出，同时通过回调推送实时块
    tracing::debug!("[ExecTool] Starting reader thread...");
    let chunk_cb = chunk_callback;
    let reader_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buf = [0u8; 8192];
        // 使用 500ms 超时轮询，避免 kill_flag 设置后 reader.read() 永远阻塞
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
                    tracing::trace!("[ExecTool] Reader thread: read {} bytes", n);
                    output.extend_from_slice(&buf[..n]);
                    // 推送实时块（清洗 ANSI 后）
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
        tracing::debug!("[ExecTool] Reader thread finished, total: {} bytes", output.len());
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

        if last_warn_time.elapsed() > Duration::from_secs(5) {
            let elapsed = timeout_secs.saturating_sub(deadline.saturating_duration_since(Instant::now()).as_secs());
            tracing::debug!("[ExecTool] Waiting for command... elapsed: ~{}s", timeout_secs.saturating_sub(elapsed));
            last_warn_time = Instant::now();
        }

        if let Ok(Some(_status)) = child.try_wait() {
            tracing::debug!("[ExecTool] Process exited");
            break;
        }

        std::thread::sleep(Duration::from_secs(1));
    }

    // 关键：关闭 PTY master 写端，触发 reader 收到 EOF → reader 线程自然退出
    kill_flag.store(true, Ordering::Relaxed);
    let _ = child.kill();
    let _ = child.wait();
    // 释放 PTY master 句柄，否则 Windows 下 reader 收不到 EOF 会一直阻塞
    drop(pair);
    // 给 reader 线程一点时间处理 EOF
    std::thread::sleep(Duration::from_millis(200));

    // 等待读取线程完成
    let raw_output = match reader_thread.join() {
        Ok(out) => {
            tracing::debug!("[ExecTool] Reader thread joined, output size: {} bytes", out.len());
            out
        }
        Err(_) => {
            tracing::error!("[ExecTool] Reader thread panicked!");
            Vec::new()
        }
    };

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
/// 在独立线程 + 独立 tokio runtime 中执行 PTY 操作，
/// 避免阻塞 tokio worker 线程。
pub fn execute_command_safe(
    command: &str,
    workdir: &std::path::Path,
    timeout_secs: u64,
    chunk_callback: Option<Box<dyn Fn(String) + Send>>,
    deny_patterns: &[String],
) -> CommandOutput {
    let command = command.to_string();
    let workdir = workdir.to_path_buf();
    let timeout_secs = timeout_secs.min(300);
    // 如果未传入模式，从缓存/配置加载
    let patterns: Vec<String> = if deny_patterns.is_empty() {
        load_deny_patterns()
    } else {
        deny_patterns.to_vec()
    };

    tracing::info!(
        "[ExecTool] execute_command_safe: '{}' | workdir: {} | timeout: {}s",
        command,
        workdir.display(),
        timeout_secs,
    );

    std::thread::spawn(move || {
        let result = execute_sync(&command, &workdir, timeout_secs, chunk_callback, &patterns);
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
