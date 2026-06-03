use axum::{
    extract::Path,
    routing::{delete, get, post, put},
    Json, Router,
};
use crate::APP_STATE;

/// 列出所有技能
async fn list_skills() -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let mut skills = state.skills_loader.list_skills();
    crate::skills::loader::SkillsLoader::apply_enabled_states(&mut skills, &state.config.skills);
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
    let mut skill = state.skills_loader.get_skill(&id);
    if let Some(ref mut s) = skill {
        if let Some(&enabled) = state.config.skills.get(&s.name) {
            s.enabled = enabled;
        }
    }
    match skill {
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
    let mut skill_names: Vec<String> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_path = match entry.name().to_owned() {
            name if name.ends_with('/') => continue, // 跳过目录条目
            name => name,
        };

        let path = std::path::Path::new(&entry_path);

        // 提取技能名称：ZIP 根目录名
        let skill_name = path
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("");

        // 跳过扁平 ZIP（无根目录）和根目录名为空
        if skill_name.is_empty()
            || skill_name == path.file_name().and_then(|n| n.to_str()).unwrap_or("")
        {
            continue;
        }

        // 读取文件内容（只读一次）
        let entry_data: Vec<u8> = {
            let mut buf = Vec::new();
            if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
                errors.push(format!("{}: 读取失败", entry_path));
                continue;
            }
            buf
        };

        let is_skill_md = path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "SKILL.md")
            .unwrap_or(false);

        // 如果是 SKILL.md，验证格式
        if is_skill_md {
            let content = String::from_utf8_lossy(&entry_data);
            if crate::skills::loader::SkillsLoader::parse_skill_md_raw(&content).is_none() {
                errors.push(format!("{}: SKILL.md 格式无效（缺少 name 字段或无内容）", entry_path));
                continue;
            }
        }

        // 构建目标路径，保留子目录结构
        // weather_query/scripts/run.sh → skills_dir/weather_query/scripts/run.sh
        let relative_path = path.strip_prefix(skill_name).unwrap_or(path);
        let target_path = skills_dir.join(skill_name).join(relative_path);

        // 创建父目录
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        if let Err(e) = std::fs::write(&target_path, &entry_data) {
            errors.push(format!("{}: 写入失败: {}", entry_path, e));
            continue;
        }

        if is_skill_md {
            installed_count += 1;
            skill_names.push(skill_name.to_string());
        }
    }

    // 清理：如果没有安装成功，删除已创建的空目录
    if installed_count == 0 {
        for name in &skill_names {
            let dir = skills_dir.join(name);
            let _ = std::fs::remove_dir_all(&dir);
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

/// 切换技能的启用/停用状态（存储在全局 config 的 skills map 中）
async fn toggle_skill(Path(id): Path<String>) -> Json<serde_json::Value> {
    let mut state = APP_STATE.write().await;
    // 先验证技能是否存在
    if state.skills_loader.get_skill(&id).is_none() {
        return Json(serde_json::json!({ "success": false, "message": "技能未找到" }));
    }
    let current = state.config.skills.get(&id).copied().unwrap_or(true);
    let new_state = !current;
    state.config.skills.insert(id.clone(), new_state);
    if let Err(e) = state.config.save() {
        return Json(serde_json::json!({ "success": false, "message": format!("保存配置失败: {}", e) }));
    }
    Json(serde_json::json!({
        "success": true,
        "data": { "enabled": new_state }
    }))
}

pub fn routes() -> Router {
    Router::new()
        .route("/skills", get(list_skills))
        .route("/skills/upload", post(upload_skill))
        .route("/skills/:id", get(get_skill))
        .route("/skills/:id", delete(delete_skill))
        .route("/skills/:id/toggle", put(toggle_skill))
}
