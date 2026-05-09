use axum::{
    extract::Path,
    routing::{delete, get, post},
    Json, Router,
};
use crate::APP_STATE;

/// 列出所有技能
async fn list_skills() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let skills = state.skills_loader.list_skills();
    // 转换为前端期望格式
    let skill_list: Vec<serde_json::Value> = skills
        .into_iter()
        .map(|s| serde_json::json!({
            "id": s.name,
            "name": s.name,
            "description": s.description,
            "version": s.version,
            "level": 0,
            "enabled": s.enabled,
            "content": s.content,
        }))
        .collect();

    Json(serde_json::json!({ "success": true, "data": skill_list }))
}

/// 获取指定技能
async fn get_skill(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.skills_loader.get_skill(&id) {
        Some(skill) => Json(serde_json::json!({
            "success": true,
            "data": {
                "id": skill.name,
                "name": skill.name,
                "description": skill.description,
                "version": skill.version,
                "level": 0,
                "enabled": skill.enabled,
                "content": skill.content,
            }
        })),
        None => Json(serde_json::json!({ "success": false, "message": "技能未找到" })),
    }
}

/// 删除技能
async fn delete_skill(Path(id): Path<String>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    match state.skills_loader.delete_skill(&id) {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "message": e })),
    }
}

/// 上传并安装技能（接受 .zip 文件，自动解压到 skills 目录）
async fn upload_skill(
    body: axum::body::Bytes,
) -> Json<serde_json::Value> {
    if body.is_empty() {
        return Json(serde_json::json!({ "success": false, "message": "上传内容为空" }));
    }

    let state = APP_STATE.read().await;
    let skills_dir = state.config.skills_dir();
    drop(state);

    // 读取 zip 文件
    let cursor = std::io::Cursor::new(&body);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(e) => return Json(serde_json::json!({ "success": false, "message": format!("ZIP 文件解析失败: {}", e) })),
    };

    // 确保技能目录存在
    std::fs::create_dir_all(&skills_dir).ok();

    let mut installed_count = 0;
    let mut errors: Vec<String> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // 只处理 SKILL.md 文件和目录结构
        let entry_path = match entry.name().to_owned() {
            name if name.ends_with('/') => continue, // 跳过目录条目
            name => name,
        };

        // 提取技能名称：从路径中取第一级目录名
        let path = std::path::Path::new(&entry_path);
        let skill_name = match path.parent() {
            Some(parent) => parent.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(""),
            None => "",
        };

        if skill_name.is_empty() {
            continue;
        }

        let is_skill_md = path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "SKILL.md")
            .unwrap_or(false);

        // 构建目标路径
        let target_dir = skills_dir.join(skill_name);
        let target_path = target_dir.join(path.file_name().unwrap());

        // 创建目录
        std::fs::create_dir_all(target_dir).ok();

        // 如果是 SKILL.md，尝试解析验证
        if is_skill_md {
            let mut content = String::new();
            if std::io::Read::read_to_string(&mut entry, &mut content).is_err() {
                errors.push(format!("{}: 读取失败", entry_path));
                continue;
            }

            // 验证 SKILL.md 格式（必须有 name 字段）
            let skill = crate::skills::loader::SkillsLoader::parse_skill_md_raw(&content);
            if skill.is_none() {
                errors.push(format!("{}: SKILL.md 格式无效（缺少 name 字段或无内容）", entry_path));
                continue;
            }
        }

        // 写入文件
        let entry_data: Vec<u8> = {
            let mut buf = Vec::new();
            if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
                errors.push(format!("{}: 读取失败", entry_path));
                continue;
            }
            buf
        };
        if let Err(e) = std::fs::write(&target_path, &entry_data) {
            errors.push(format!("{}: 写入失败: {}", entry_path, e));
            continue;
        }

        // 确保目标路径与技能名称一致
        if is_skill_md {
            installed_count += 1;
        }
    }

    // 清理：如果没有任何技能安装成功，删除可能已创建的空目录
    if installed_count == 0 {
        for i in 0..archive.len() {
            let entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = std::path::Path::new(entry.name());
            if let Some(parent) = path.parent() {
                if let Some(name) = parent.file_name().and_then(|n| n.to_str()) {
                    let dir = skills_dir.join(name);
                    let _ = std::fs::remove_dir_all(&dir);
                }
            }
        }
    }

    if installed_count == 0 && !errors.is_empty() {
        return Json(serde_json::json!({
            "success": false,
            "message": format!("技能安装失败: {}", errors.join("; ")),
        }));
    }

    Json(serde_json::json!({
        "success": true,
        "data": {
            "installed": installed_count,
            "errors": errors,
        },
        "message": format!("成功安装 {} 个技能", installed_count),
    }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/skills", get(list_skills))
        .route("/skills/upload", post(upload_skill))
        .route("/skills/{id}", get(get_skill))
        .route("/skills/{id}", delete(delete_skill))
}
