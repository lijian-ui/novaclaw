use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;

use novaclaw_backend::APP_STATE;

/// 文件条目（用于详细目录列表）
#[derive(Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub extension: String,
}

/// 读取本地文件内容（支持非 UTF-8 文件，自动降级为 lossy 表示）
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("读取文件失败: {}", e))?;
    // 使用 lossy 转换，非 UTF-8 字节替换为 ?
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// 写入本地文件内容
#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::write(&path, &content).map_err(|e| format!("写入文件失败: {}", e))
}

/// 创建目录
#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| format!("创建目录失败: {}", e))
}

/// 删除文件或空目录
#[tauri::command]
pub async fn delete_path(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err("路径不存在".to_string());
    }
    if p.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("删除目录失败: {}", e))
    } else {
        std::fs::remove_file(&path).map_err(|e| format!("删除文件失败: {}", e))
    }
}

/// 重命名文件或目录
#[tauri::command]
pub async fn rename_path(old_path: String, new_path: String) -> Result<(), String> {
    std::fs::rename(&old_path, &new_path).map_err(|e| format!("重命名失败: {}", e))
}

/// 复制文件或目录（递归）
#[tauri::command]
pub async fn copy_path(source: String, dest: String) -> Result<(), String> {
    let src = Path::new(&source);
    let dst = Path::new(&dest);
    if src.is_dir() {
        copy_dir_recursive(src, dst).map_err(|e| format!("复制目录失败: {}", e))
    } else {
        std::fs::copy(src, dst).map_err(|e| format!("复制文件失败: {}", e))?;
        Ok(())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// 读取目录列表（简略版，返回 "名称 (dir/file)" 格式）
#[tauri::command]
pub async fn list_directory(path: String) -> Result<Vec<String>, String> {
    let mut entries = Vec::new();
    match std::fs::read_dir(&path) {
        Ok(dir) => {
            for entry in dir.flatten() {
                let file_type = entry.file_type().map(|ft| {
                    if ft.is_dir() { "dir" } else { "file" }
                }).unwrap_or("unknown");
                let name = entry.file_name().to_string_lossy().to_string();
                entries.push(format!("{} ({})", name, file_type));
            }
        }
        Err(e) => return Err(format!("读取目录失败: {}", e)),
    }
    Ok(entries)
}

/// 获取应用配置（从文件重新加载后返回 JSON 字符串）
#[tauri::command]
pub async fn get_config_json() -> Result<String, String> {
    let fresh_config = novaclaw_backend::config::AppConfig::reload();
    {
        let mut state = APP_STATE.write().await;
        state.config = fresh_config.clone();
    }
    serde_json::to_string(&fresh_config).map_err(|e| format!("序列化错误: {}", e))
}

/// 保存应用配置
#[tauri::command]
pub async fn save_config_json(config_json: String) -> Result<(), String> {
    let config: novaclaw_backend::config::AppConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("反序列化错误: {}", e))?;
    let mut state = APP_STATE.write().await;
    state.config = config;
    state.config.save().map_err(|e| format!("保存失败: {}", e))?;
    let reloaded = novaclaw_backend::config::AppConfig::reload();
    state.config = reloaded;
    tracing::info!("Tauri 项目配置已保存并重新加载");
    Ok(())
}

/// 获取模型配置（从文件重新加载后返回 JSON 字符串）
#[tauri::command]
pub async fn get_models_json() -> Result<String, String> {
    let fresh_config = novaclaw_backend::config::ModelsConfig::reload();
    {
        let mut state = APP_STATE.write().await;
        state.models_config = fresh_config.clone();
    }
    serde_json::to_string(&fresh_config).map_err(|e| format!("序列化错误: {}", e))
}

/// 保存模型配置
#[tauri::command]
pub async fn save_models_json(models_json: String) -> Result<(), String> {
    let config: novaclaw_backend::config::ModelsConfig =
        serde_json::from_str(&models_json).map_err(|e| format!("反序列化错误: {}", e))?;
    let mut state = APP_STATE.write().await;
    state.models_config = config;
    state.models_config.save().map_err(|e| format!("保存失败: {}", e))?;
    let reloaded = novaclaw_backend::config::ModelsConfig::reload();
    state.models_config = reloaded;
    tracing::info!("Tauri 模型配置已保存并重新加载");
    Ok(())
}

/// 获取数据目录（基础目录）
#[tauri::command]
pub async fn get_data_dir() -> Result<String, String> {
    Ok(novaclaw_backend::config::get_base_dir().to_string_lossy().to_string())
}

/// 获取配置目录
#[tauri::command]
pub async fn get_config_dir() -> Result<String, String> {
    Ok(novaclaw_backend::config::get_config_dir().to_string_lossy().to_string())
}

/// 获取工作目录
#[tauri::command]
pub async fn get_workspace_dir() -> Result<String, String> {
    Ok(novaclaw_backend::config::get_workspace_dir().to_string_lossy().to_string())
}

/// 获取技能目录
#[tauri::command]
pub async fn get_skills_dir() -> Result<String, String> {
    Ok(novaclaw_backend::config::get_skills_dir().to_string_lossy().to_string())
}

/// 获取记忆目录
#[tauri::command]
pub async fn get_memories_dir() -> Result<String, String> {
    Ok(novaclaw_backend::config::get_memories_dir().to_string_lossy().to_string())
}

/// 获取会话目录
#[tauri::command]
pub async fn get_sessions_dir() -> Result<String, String> {
    Ok(novaclaw_backend::config::get_sessions_dir().to_string_lossy().to_string())
}

/// 获取系统信息
#[tauri::command]
pub fn get_system_info() -> serde_json::Value {
    serde_json::json!({
        "os": if cfg!(target_os = "windows") { "windows" }
              else if cfg!(target_os = "macos") { "macos" }
              else { "linux" },
        "arch": if cfg!(target_arch = "x86_64") { "x86_64" }
                else if cfg!(target_arch = "aarch64") { "aarch64" }
                else { "unknown" },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
}

/// 显示主窗口
#[tauri::command]
pub async fn show_window(window: tauri::Window) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    Ok(())
}

/// 隐藏主窗口到托盘
#[tauri::command]
pub async fn hide_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

/// 最小化窗口
#[tauri::command]
pub async fn minimize_window(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

/// 最大化/还原窗口
#[tauri::command]
pub async fn maximize_window(window: tauri::Window) -> Result<(), String> {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

/// 关闭窗口（隐藏到托盘）
#[tauri::command]
pub async fn close_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

/// 读取目录列表（详细版，返回结构化文件条目）
#[tauri::command]
pub async fn list_directory_detailed(path: String) -> Result<Vec<FileEntry>, String> {
    let dir_path = Path::new(&path);
    let mut entries = Vec::new();

    match std::fs::read_dir(dir_path) {
        Ok(dir) => {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name == "node_modules" {
                    continue;
                }
                let full_path = entry.path();
                let path_str = full_path.to_string_lossy().to_string();
                let metadata = entry.metadata().ok();
                let is_dir = full_path.is_dir();
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata
                    .and_then(|m| m.modified().ok())
                    .map(|t| {
                        let dt: chrono::DateTime<chrono::Utc> = t.into();
                        dt.to_rfc3339()
                    })
                    .unwrap_or_default();
                let extension = full_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();

                entries.push(FileEntry {
                    name,
                    path: path_str,
                    is_dir,
                    size,
                    modified,
                    extension,
                });
            }
        }
        Err(e) => return Err(format!("读取目录失败: {}", e)),
    }

    // 目录在前，文件在后
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    Ok(entries)
}

// ============================================================
// 终端进程管理（Tauri 桌面端）- 持久化 Shell 模式
// ============================================================

/// 当前正在运行的终端进程
struct RunningProcess {
    child: Child,
    stdin: tokio::sync::mpsc::Sender<String>,
    killed: Arc<AtomicBool>,
}

static TERMINAL_PROCESS: once_cell::sync::Lazy<tokio::sync::Mutex<Option<RunningProcess>>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(None));

/// 检测当前操作系统并返回默认终端 shell
fn detect_shell() -> (&'static str, &'static [&'static str]) {
    if cfg!(target_os = "windows") {
        ("cmd.exe", &["/Q"] as &[&str])
    } else if cfg!(target_os = "macos") {
        ("/bin/zsh", &["-i"] as &[&str])
    } else {
        ("/bin/bash", &["--login"] as &[&str])
    }
}

/// 启动持久化终端进程（Tauri 桌面端）
#[tauri::command]
pub async fn terminal_spawn(app: tauri::AppHandle) -> Result<(), String> {
    let mut guard = TERMINAL_PROCESS.lock().await;
    
    if guard.is_some() {
        return Ok(());
    }

    let (shell, shell_args) = detect_shell();
    
    let mut child = tokio::process::Command::new(shell)
        .args(shell_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("启动 Shell 失败: {}", e))?;

    let stdin_pipe = child.stdin.take().ok_or("无法获取 stdin")?;
    let stdout_pipe = child.stdout.take().ok_or("无法获取 stdout")?;
    let stderr_pipe = child.stderr.take().ok_or("无法获取 stderr")?;
    let killed = Arc::new(AtomicBool::new(false));
    let killed_clone = killed.clone();
    let killed_out = killed.clone();
    let killed_err = killed.clone();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);

    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut writer = tokio::io::BufWriter::new(stdin_pipe);
        while let Some(data) = rx.recv().await {
            let _ = writer.write_all(data.as_bytes()).await;
            let _ = writer.flush().await;
        }
    });

    let app_stdout = app.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout_pipe);
        let mut buf = [0u8; 8192];
        loop {
            if killed_out.load(Ordering::Relaxed) {
                break;
            }
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_stdout.emit("terminal:stdout", text);
                }
                Err(_) => break,
            }
        }
    });

    let app_stderr = app.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr_pipe);
        let mut buf = [0u8; 8192];
        loop {
            if killed_err.load(Ordering::Relaxed) {
                break;
            }
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_stderr.emit("terminal:stderr", text);
                }
                Err(_) => break,
            }
        }
    });

    *guard = Some(RunningProcess {
        child,
        stdin: tx,
        killed,
    });

    // Shell 启动后设置提示符格式并显示
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // 设置简洁的提示符格式
    {
        let mut guard = TERMINAL_PROCESS.lock().await;
        if let Some(proc) = guard.as_mut() {
            proc.stdin.send("prompt $P$G ".to_string() + "\r\n").await.ok();
        }
    }
    
    // 短暂延迟后发送空行触发提示符显示
    let app_prompt = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        let _ = app_prompt.emit("terminal:stdout", "\r\n".to_string());
    });
    
    let _ = app.emit("terminal:connected", "");
    Ok(())
}

/// 执行终端命令（持久化 Shell 模式下发送命令）
#[tauri::command]
pub async fn terminal_exec(app: tauri::AppHandle, command: String) -> Result<(), String> {
    let mut guard = TERMINAL_PROCESS.lock().await;
    
    match guard.as_mut() {
        Some(proc) => {
            // 清屏命令特殊处理
            if command.trim() == "clear" || command.trim() == "cls" {
                let _ = app.emit("terminal:clear", "");
                // 清屏后发送新的提示符
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                let _ = app.emit("terminal:stdout", get_windows_prompt());
                return Ok(());
            }
            
            if !command.is_empty() {
                proc.stdin.send(command.trim_end().to_string() + "\r\n").await.map_err(|e| format!("发送命令失败: {}", e))?;
                
                // 命令执行后发送新的提示符（延迟等待命令执行完成）
                let app_clone = app.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    let _ = app_clone.emit("terminal:stdout", get_windows_prompt());
                });
            } else {
                // 空命令也显示提示符
                let _ = app.emit("terminal:stdout", get_windows_prompt());
            }
            Ok(())
        }
        None => {
            drop(guard);
            terminal_spawn(app.clone()).await?;
            
            // Shell 启动后设置提示符格式并显示
            let mut guard = TERMINAL_PROCESS.lock().await;
            if let Some(proc) = guard.as_mut() {
                // 发送设置提示符命令
                proc.stdin.send("prompt $P$G".to_string() + "\r\n").await.ok();
                
                // 短暂延迟后发送空行触发提示符显示
                drop(guard);
                let app_clone = app.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    let _ = app_clone.emit("terminal:stdout", "\r\n".to_string());
                });
            }
            
            // 如果有命令则执行
            if !command.trim().is_empty() {
                let app_exec = app.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                    let mut guard = TERMINAL_PROCESS.lock().await;
                    if let Some(proc) = guard.as_mut() {
                        let _ = proc.stdin.send(command.trim().to_string() + "\r\n").await;
                    }
                });
            }
            Ok(())
        }
    }
}

/// 获取 Windows 提示符（当前路径格式）
fn get_windows_prompt() -> String {
    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd.to_string_lossy();
        format!("\r\n{}>", path)
    } else {
        "\r\n>".to_string()
    }
}

/// 向当前进程 stdin 写入数据（用于交互式终端）
#[tauri::command]
pub async fn terminal_write(data: String) -> Result<(), String> {
    let mut guard = TERMINAL_PROCESS.lock().await;
    match guard.as_mut() {
        Some(proc) => {
            proc.stdin.send(data).await.map_err(|e| format!("写入 stdin 失败: {}", e))?;
            Ok(())
        }
        None => {
            drop(guard);
            Err("终端未启动".to_string())
        }
    }
}

/// 终止当前运行中的终端进程
#[tauri::command]
pub async fn terminal_kill() -> Result<(), String> {
    let mut guard = TERMINAL_PROCESS.lock().await;
    if let Some(mut proc) = guard.take() {
        proc.killed.store(true, Ordering::Relaxed);
        let _ = proc.child.kill().await;
    }
    Ok(())
}

/// 调整终端尺寸
#[tauri::command]
pub async fn terminal_resize(_cols: u16, _rows: u16) -> Result<(), String> {
    Ok(())
}
