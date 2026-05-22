use crate::tools::builtin::resolve_path;
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 execute_command 相关工具
pub async fn register(registry: &ToolRegistry) {
    // ── execute_command: 同步执行（原地等待结果） ──
    registry
        .register(ToolDef {
            name: "execute_command".to_string(),
            description: "Execute a shell command (quick, returns result directly). For long-running commands, use execute_command_bg. Params: command (required), description (optional), timeout (default 60s, max 300), workdir (optional)".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute (e.g. 'npm run build', 'cargo test', 'python script.py')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Clear explanation of what this command does (helps with safety review)"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Maximum execution time in seconds (default 60, max 300)"
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory (relative to workspace or absolute, defaults to workspace)"
                    }
                },
                "required": ["command"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let command = args["command"].as_str().ok_or("Missing 'command' parameter")?;
                    let timeout = args
                        .get("timeout")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(60)
                        .min(300);
                    let workdir_str = args.get("workdir").and_then(|v| v.as_str()).unwrap_or(".");
                    let resolved_workdir = resolve_path(workdir_str, &args);
                    tracing::info!(
                        "[Execute] 执行命令: {} | 工作目录: {}",
                        command,
                        resolved_workdir.display()
                    );

                    if !resolved_workdir.exists() {
                        return Err(format!(
                            "Working directory not found: {}",
                            resolved_workdir.display()
                        ));
                    }

                    let chunk_cb = chunk_tx.map(|tx| {
                        let tx_clone = tx.clone();
                        Box::new(move |chunk: String| {
                            let _ = tx_clone.send(chunk);
                        }) as Box<dyn Fn(String) + Send>
                    });

                    let result = crate::tools::execute::execute_command_safe(
                        command,
                        &resolved_workdir,
                        timeout,
                        chunk_cb,
                        &[],
                    );

                    if result.blocked {
                        return Err(format!(
                            "⛔ 命令被安全策略拦截（匹配黑名单模式: {}）\n\n\
                             这个命令已被系统设置为禁止执行，不是执行出错。\n\
                             如果你认为这个命令是安全的，请在「设置 → 安全」中移除对应的黑名单关键词后再试。\n\
                             注意：\n\
                             - 这是安全策略限制，不是命令执行失败\n\
                             - 不要尝试使用其他同义命令绕过限制",
                            result.block_reason
                        ));
                    }

                    let mut output = String::new();
                    output.push_str(&result.stdout);

                    if let Some(code) = result.exit_code {
                        if code != 0 {
                            output.push_str(&format!("\n\n[Exit code: {}]", code));
                        }
                    }
                    if result.timed_out {
                        output.push_str(&format!("\n\n[Command timed out after {}s]", timeout));
                    }

                    Ok(output)
                },
            ),
        })
        .await;

    // ── execute_command_bg: 后台执行（立即返回 task_id） ──
    registry
        .register(ToolDef {
            name: "execute_command_bg".to_string(),
            description: "Execute a shell command in BACKGROUND, returns a task_id immediately. Use for long-running commands. Check result later with poll_command. Params: command (required), description (optional), workdir (optional)".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute in background (e.g. 'npm install', 'cargo build')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Clear explanation of what this command does"
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory (relative to workspace or absolute, defaults to workspace)"
                    }
                },
                "required": ["command"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let command = args["command"].as_str().ok_or("Missing 'command' parameter")?;
                    let workdir_str = args.get("workdir").and_then(|v| v.as_str()).unwrap_or(".");
                    let resolved_workdir = resolve_path(workdir_str, &args);
                    tracing::info!(
                        "[Execute:BG] 提交后台命令: {} | 工作目录: {}",
                        command,
                        resolved_workdir.display()
                    );

                    if !resolved_workdir.exists() {
                        return Err(format!(
                            "Working directory not found: {}",
                            resolved_workdir.display()
                        ));
                    }

                    let task_id = crate::bg_task::submit(
                        command,
                        resolved_workdir.clone(),
                        600, // 后台命令超时 10 分钟
                    );

                    Ok(format!(
                        "后台命令已提交，Task ID: {}\n命令: {}\n\n你可以继续做其他工作，稍后调用 poll_command(task_id=\"{}\") 查看执行结果。",
                        task_id, command, task_id
                    ))
                },
            ),
        })
        .await;

    // ── poll_command: 查询后台命令结果 ──
    registry
        .register(ToolDef {
            name: "poll_command".to_string(),
            description: "Check the status of a background command. Params: task_id (required)".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID returned by execute_command_bg"
                    }
                },
                "required": ["task_id"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let task_id = args["task_id"]
                        .as_str()
                        .ok_or("Missing 'task_id' parameter")?;

                    match crate::bg_task::query(task_id) {
                        Some(task) => {
                            let status_str = match &task.status {
                                crate::bg_task::BgTaskStatus::Running => {
                                    format!("⏳ 运行中（当前输出 {} 字符）", task.stdout.len())
                                }
                                crate::bg_task::BgTaskStatus::Done => "✅ 已完成".to_string(),
                                crate::bg_task::BgTaskStatus::Failed(e) => {
                                    format!("❌ 执行失败: {}", e)
                                }
                            };
                            let exit_info = match task.exit_code {
                                Some(0) => "\n退出码: 0 (成功)".to_string(),
                                Some(c) => format!("\n退出码: {}", c),
                                None => String::new(),
                            };

                            // 如果还在运行，提醒 LLM 继续干活
                            let running_hint = match &task.status {
                                crate::bg_task::BgTaskStatus::Running => {
                                    "\n\n建议继续做其他工作，过一会儿再调用 poll_command 查看结果。"
                                }
                                _ => "",
                            };

                            Ok(format!(
                                "任务: {}\n命令: {}\n状态: {}{}\n\n{}{}",
                                task.id, task.command, status_str, exit_info, task.stdout, running_hint
                            ))
                        }
                        None => Err(format!(
                            "未找到任务 '{}'。task_id 是否正确？可能任务已被清理。",
                            task_id
                        )),
                    }
                },
            ),
        })
        .await;
}
