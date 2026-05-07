use novaclaw_backend::APP_STATE;

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

/// 读取本地文件内容
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {}", e))
}

/// 写入本地文件内容
#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::write(&path, &content).map_err(|e| format!("写入文件失败: {}", e))
}

/// 读取目录列表
#[tauri::command]
pub async fn list_directory(path: String) -> Result<Vec<String>, String> {
    let mut entries = Vec::new();
    match std::fs::read_dir(&path) {
        Ok(dir) => {
            for entry in dir.flatten() {
                let file_type = entry.file_type().map(|ft| {
                    if ft.is_dir() {
                        "dir"
                    } else {
                        "file"
                    }
                }).unwrap_or("unknown");
                let name = entry.file_name().to_string_lossy().to_string();
                entries.push(format!("{} ({})", name, file_type));
            }
        }
        Err(e) => return Err(format!("读取目录失败: {}", e)),
    }
    Ok(entries)
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
        "os": if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        },
        "arch": if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "unknown"
        },
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
