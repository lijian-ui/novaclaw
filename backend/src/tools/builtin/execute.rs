use crate::tools::builtin::resolve_path;
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 execute_command 相关工具
pub async fn register(registry: &ToolRegistry) {
    // ── execute_command: 同步执行（原地等待结果） ──
    registry
        .register(ToolDef {
                        name: "execute_command".to_string(),
            display_name: "执行命令".to_string(),
            description: "Execute a shell command and wait for the result. Params: command (required), description (optional), workdir (optional)".to_string(),
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
                    "workdir": {
                        "type": "string",
                        "description": "Working directory (relative to working directory or absolute, defaults to current directory)"
                    }
                },
                "required": ["command"]
            }),
            skip_truncation_save: false,
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let command = args["command"].as_str().ok_or("Missing 'command' parameter")?;
                    let timeout = 36000u64; // 命令等待完成，不设严格超时（由 registry 层 10 小时兜底）
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

                    // ── 安全审批流程 ──
                    // 1. 检查黑名单（高危命令直接拒绝）
                    let deny_patterns = crate::tools::execute::load_deny_patterns();
                    if let Some(pattern) = crate::tools::execute::check_command_deny(command, &deny_patterns) {
                        return Err(format!(
                            "⛔ 命令被安全策略拦截（匹配黑名单: {}）\n\n\
                             这个命令已被系统设置为禁止执行。",
                            pattern
                        ));
                    }
                    // 2. 检查审批模式
                    let approval_mode = crate::tools::execute::load_approval_mode();
                    if approval_mode == "auto" {
                        // auto 模式：跳过审批，直接执行（仍受黑名单限制）
                    } else if crate::tools::execute::check_command_allow(command) {
                        // 审批模式下，白名单命令直接执行
                    } else {
                        // 3. 审批模式 + 不在白名单 → 返回 PendingApproval 等待用户确认
                        let cmd_str = command.to_string();
                        let description = args.get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let approval_json = serde_json::json!({
                            "__type": "PendingApproval",
                            "approval": {
                                "operation_type": "execute",
                                "tool_name": "execute_command",
                                "arguments": cmd_str,
                                "message": format!(
                                    "允许执行命令吗？\n\n命令: {}\n说明: {}",
                                    cmd_str,
                                    if description.is_empty() { "无" } else { &description }
                                ),
                                "affected_files": []
                            }
                        });
                        return Ok(approval_json.to_string());
                    }

                    let chunk_cb = chunk_tx.map(|tx| {
                        let tx_clone = tx.clone();
                        Box::new(move |chunk: String| {
                            let _ = tx_clone.send(chunk);
                        }) as Box<dyn Fn(String) + Send>
                    });

                    // 创建取消信号：从 args 中提取 session_id 并查找对应的 cancel flag
                    let cancel_flag = args.get("_session_id")
                        .and_then(|v| v.as_str())
                        .and_then(|sid| {
                            crate::APP_STATE.try_read().ok().and_then(|state| {
                                state.cancel_map.get(sid).cloned()
                            })
                        });

                    let result = crate::tools::execute::execute_command_safe(
                        command,
                        &resolved_workdir,
                        timeout,
                        chunk_cb,
                        &[],
                        cancel_flag.clone(),
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

                    // 截断 stdout：采用 Reasonix 式的头尾保留策略
                    // 保留前 60% + 后 30%，中间插入截断标记，让 LLM 既能看头也能看尾
                    const MAX_OUTPUT_CHARS: usize = 32000;
                    let stdout = &result.stdout;
                    let truncated = if stdout.len() > MAX_OUTPUT_CHARS {
                        let head_chars = (MAX_OUTPUT_CHARS as f64 * 0.60) as usize;
                        let tail_chars = (MAX_OUTPUT_CHARS as f64 * 0.30) as usize;
                        let mut head_end = head_chars.min(stdout.len());
                        while head_end > 0 && !stdout.is_char_boundary(head_end) { head_end -= 1; }
                        let mut tail_start = stdout.len().saturating_sub(tail_chars);
                        while tail_start < stdout.len() && !stdout.is_char_boundary(tail_start) { tail_start += 1; }
                        let head = &stdout[..head_end];
                        let tail = &stdout[tail_start..];
                        format!(
                            "{}\n\n[… 中间 {} 字符已截断 …]\n\n{}",
                            head,
                            stdout.len() - head_end - (stdout.len() - tail_start),
                            tail
                        )
                    } else {
                        stdout.to_string()
                    };

                    let mut output = truncated;
                    // 参考 Reasonix formatCommandResult: 始终显示 [exit N] 标记
                    // 即使 exit code = 0 也显示，让 LLM 始终知道命令执行状态
                    if let Some(code) = result.exit_code {
                        output.push_str(&format!("\n\n[exit {}]", code));
                    } else {
                        output.push_str("\n\n[exit ?]");
                    }
                    if result.timed_out {
                        // 区分"被取消"和"超时"
                        let is_cancelled = cancel_flag.as_ref()
                            .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
                            .unwrap_or(false);
                        if is_cancelled {
                            output.push_str("\n\n[命令已被用户取消]");
                        } else {
                            output.push_str(&format!("\n\n[Command timed out after {}s]", timeout));
                        }
                    }

                    Ok(output)
                },
            ),
        })
        .await;

}
