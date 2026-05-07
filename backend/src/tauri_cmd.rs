use crate::APP_STATE;

/// 获取应用配置（从文件重新加载后返回 JSON 字符串，确保数据最新）
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_config_json() -> Result<String, String> {
    // 从文件重新加载，确保返回磁盘上的最新数据
    let fresh_config = crate::config::AppConfig::reload();

    // 同步更新内存状态
    {
        let mut state = APP_STATE.write().await;
        state.config = fresh_config.clone();
    }

    serde_json::to_string(&fresh_config).map_err(|e| format!("序列化错误: {}", e))
}

/// 保存应用配置
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn save_config_json(config_json: String) -> Result<(), String> {
    let config: crate::config::AppConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("反序列化错误: {}", e))?;

    let mut state = APP_STATE.write().await;
    state.config = config;

    // 保存到文件
    state.config.save().map_err(|e| format!("保存失败: {}", e))?;

    // 重新从文件加载，确保内存状态与文件同步
    let reloaded = crate::config::AppConfig::reload();
    state.config = reloaded;

    tracing::info!("Tauri 配置已保存并重新加载");
    Ok(())
}

/// 读取本地文件内容
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {}", e))
}

/// 写入本地文件内容
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::write(&path, &content).map_err(|e| format!("写入文件失败: {}", e))
}

/// 读取目录列表
#[cfg(feature = "tauri")]
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

/// 获取数据目录
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_data_dir() -> Result<String, String> {
    let state = APP_STATE.read().await;
    Ok(state.config.data_dir().to_string_lossy().to_string())
}

/// 获取系统信息
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_system_info() -> serde_json::Value {
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

/// 注册所有 Tauri 命令
#[cfg(feature = "tauri")]
pub fn register_commands(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Tauri v2 命令通过 builder.invoke_handler 注册
    Ok(())
}
