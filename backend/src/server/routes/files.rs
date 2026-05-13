use axum::{routing::{get, post}, Json, Router};
use serde::Deserialize;
use std::path::Path;

use crate::APP_STATE;

// ─── 请求体结构 ────────────────────────────────────────────────

#[derive(Deserialize)]
struct ReadReq { path: String }

#[derive(Deserialize)]
struct WriteReq { path: String, content: String }

#[derive(Deserialize)]
struct ListReq { path: String }

#[derive(Deserialize)]
struct CopyReq { source: String, dest: String }

#[derive(Deserialize)]
struct RenameReq { old_path: String, new_path: String }

#[derive(Deserialize)]
struct DeleteReq { path: String }

#[derive(Deserialize)]
struct MkdirReq { path: String }

// ─── 文件条目 ──────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    extension: Option<String>,
    size: u64,
    modified: String,
}

// ─── 辅助函数 ──────────────────────────────────────────────────

fn list_directory(path: &str) -> Result<Vec<FileEntry>, String> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        return Err(format!("不是有效的目录: {}", path));
    }
    let mut entries = Vec::new();
    let mut read_dir = std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in read_dir.by_ref().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = entry.metadata().ok();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let ext = if is_dir {
            None
        } else {
            Path::new(&name).extension().map(|e| e.to_string_lossy().to_string())
        };
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.to_rfc3339()
            })
            .unwrap_or_default();
        entries.push(FileEntry {
            name,
            path: entry.path().to_string_lossy().to_string(),
            is_dir,
            extension: ext,
            size,
            modified,
        });
    }
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir { b.is_dir.cmp(&a.is_dir) }
        else { a.name.cmp(&b.name) }
    });
    Ok(entries)
}

// ─── 端点 ──────────────────────────────────────────────────────

/// 读取文件内容
async fn read_file(Json(req): Json<ReadReq>) -> Json<serde_json::Value> {
    match std::fs::read_to_string(&req.path) {
        Ok(content) => Json(serde_json::json!({ "success": true, "data": content })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": format!("读取文件失败: {}", e) })),
    }
}

/// 写入文件内容
async fn write_file(Json(req): Json<WriteReq>) -> Json<serde_json::Value> {
    if let Some(parent) = Path::new(&req.path).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Json(serde_json::json!({ "success": false, "message": format!("创建目录失败: {}", e) }));
            }
        }
    }
    match std::fs::write(&req.path, &req.content) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": format!("写入文件失败: {}", e) })),
    }
}

/// 列出目录内容
async fn list_dir(Json(req): Json<ListReq>) -> Json<serde_json::Value> {
    match list_directory(&req.path) {
        Ok(entries) => Json(serde_json::json!({ "success": true, "data": entries })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e })),
    }
}

/// 复制文件或目录
async fn copy_path(Json(req): Json<CopyReq>) -> Json<serde_json::Value> {
    let result = if Path::new(&req.source).is_dir() {
        copy_dir_recursive(Path::new(&req.source), Path::new(&req.dest))
    } else {
        if let Some(parent) = Path::new(&req.dest).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::copy(&req.source, &req.dest).map(|_| ()).map_err(|e| e.to_string())
    };
    match result {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": format!("复制失败: {}", e) })),
    }
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("创建目标目录失败: {}", e))?;
    let entries = std::fs::read_dir(src).map_err(|e| format!("读取源目录失败: {}", e))?;
    for entry in entries.flatten() {
        let file_type = entry.file_type().map_err(|e| format!("获取文件类型失败: {}", e))?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(&entry.path(), &dest_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

/// 重命名/移动文件或目录
async fn rename_path(Json(req): Json<RenameReq>) -> Json<serde_json::Value> {
    if let Some(parent) = Path::new(&req.new_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match std::fs::rename(&req.old_path, &req.new_path) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": format!("重命名失败: {}", e) })),
    }
}

/// 删除文件或目录
async fn delete_path(Json(req): Json<DeleteReq>) -> Json<serde_json::Value> {
    let path = Path::new(&req.path);
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    };
    match result {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": format!("删除失败: {}", e) })),
    }
}

/// 创建目录
async fn mkdir(Json(req): Json<MkdirReq>) -> Json<serde_json::Value> {
    match std::fs::create_dir_all(&req.path) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": format!("创建目录失败: {}", e) })),
    }
}

/// 获取布局（兼容旧接口）
async fn get_layout() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "data": {
            "id": "default",
            "name": "默认布局",
            "content": "{}",
            "user_id": "default",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }
    }))
}

/// 保存布局（兼容旧接口）
async fn save_layout(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "data": {
            "id": "default",
            "name": body["name"].as_str().unwrap_or("默认布局"),
            "content": body["content"].as_str().unwrap_or("{}"),
            "user_id": "default",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }
    }))
}

/// 清空本地缓存（删除 sessions 和 memories 目录内容）
async fn clear_cache() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let dirs = [
        state.config.sessions_dir(),
        state.config.memories_dir(),
    ];
    drop(state);
    for dir in &dirs {
        if !dir.exists() { continue; }
        if let Err(e) = std::fs::remove_dir_all(dir) {
            tracing::warn!("清空缓存失败 ({}): {}", dir.display(), e);
        }
        std::fs::create_dir_all(dir).ok();
    }
    tracing::info!("缓存已清空");
    Json(serde_json::json!({ "success": true, "message": "缓存已清空" }))
}

/// 获取所有目录路径
async fn get_paths() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    Json(serde_json::json!({
        "success": true,
        "data": {
            "config_dir": crate::config::get_config_dir().to_string_lossy(),
            "data_dir": state.config.data_dir().to_string_lossy(),
            "workspace_dir": state.config.workspace_dir().to_string_lossy(),
            "sessions_dir": state.config.sessions_dir().to_string_lossy(),
            "memories_dir": state.config.memories_dir().to_string_lossy(),
            "skills_dir": state.config.skills_dir().to_string_lossy(),
            "logs_dir": crate::config::get_logs_dir().to_string_lossy(),
        }
    }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/layout", get(get_layout))
        .route("/layout", post(save_layout))
        .route("/files/read", post(read_file))
        .route("/files/write", post(write_file))
        .route("/files/list", post(list_dir))
        .route("/files/copy", post(copy_path))
        .route("/files/rename", post(rename_path))
        .route("/files/delete", post(delete_path))
        .route("/files/mkdir", post(mkdir))
        .route("/cache/clear", post(clear_cache))
        .route("/paths", get(get_paths))
}
