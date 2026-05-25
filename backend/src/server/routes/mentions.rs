use axum::{routing::post, Json, Router};
use serde::Deserialize;
use tokio::fs;

use crate::APP_STATE;

/// @-mention 文件查询请求
#[derive(Deserialize)]
struct MentionQuery {
    workspace: Option<String>,
    query: String,
}

/// @-mention 展开请求
#[derive(Deserialize)]
struct ExpandMentionReq {
    content: String,
    workspace: Option<String>,
}

/// 查询目录文件列表
async fn list_mentions(Json(req): Json<MentionQuery>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let workspace_path = if let Some(ref ws) = req.workspace {
        if ws.is_empty() {
            state.config.workspace_dir()
        } else {
            std::path::PathBuf::from(ws)
        }
    } else {
        state.config.workspace_dir()
    };
    drop(state);

    if !workspace_path.exists() {
        return Json(serde_json::json!({ "success": false, "message": "工作目录不存在" }));
    }

    let query = req.query.trim().to_string();
    let search_dir = if query.is_empty() {
        workspace_path.clone()
    } else {
        let full_path = workspace_path.join(&query);
        if full_path.is_dir() {
            full_path
        } else {
            workspace_path.clone()
        }
    };

    let mut entries = Vec::new();
    let mut read_dir = match fs::read_dir(&search_dir).await {
        Ok(d) => d,
        Err(_) => return Json(serde_json::json!({ "success": false, "message": "读取目录失败" })),
    };

    let is_search = if query.is_empty() {
        false
    } else {
        !workspace_path.join(&query).is_dir()
    };
    let search_term = if is_search {
        query.split('/').last().unwrap_or(&query).to_lowercase()
    } else {
        String::new()
    };

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == ".git" {
            continue;
        }
        let path = entry.path();
        let is_dir = path.is_dir();
        let rel_path = pathdiff::diff_paths(&path, &workspace_path)
            .unwrap_or_else(|| path.clone())
            .to_string_lossy()
            .to_string();

        if is_search && !name.to_lowercase().contains(&search_term) {
            continue;
        }

        entries.push(serde_json::json!({
            "name": name,
            "path": rel_path,
            "is_dir": is_dir,
        }));
    }

    entries.sort_by(|a, b| {
        let a_dir = a["is_dir"].as_bool().unwrap_or(false);
        let b_dir = b["is_dir"].as_bool().unwrap_or(false);
        if a_dir != b_dir { b_dir.cmp(&a_dir) }
        else { a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")) }
    });

    Json(serde_json::json!({ "success": true, "data": entries }))
}

/// 展开 @ 引用 - 仅转换为 <file path> / <directory> 标记，不读内容
async fn expand_mentions(Json(req): Json<ExpandMentionReq>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let workspace_path = if let Some(ref ws) = req.workspace {
        if ws.is_empty() {
            state.config.workspace_dir()
        } else {
            std::path::PathBuf::from(ws)
        }
    } else {
        state.config.workspace_dir()
    };
    drop(state);

    let re = regex::Regex::new(r"@([^\s，。、；：）\)]+)")
        .expect("Invalid mention regex");
    let result = re.replace_all(&req.content, |caps: &regex::Captures| {
        let path_str = &caps[1];
        let full_path = workspace_path.join(path_str);

        if full_path.is_dir() {
            format!("<directory path=\"{}\" />", path_str)
        } else if full_path.is_file() {
            format!("<file path=\"{}\" />", path_str)
        } else {
            format!("@{}（文件不存在）", path_str)
        }
    });

    Json(serde_json::json!({ "success": true, "data": result.to_string() }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/mentions", post(list_mentions))
        .route("/mentions/expand", post(expand_mentions))
}
