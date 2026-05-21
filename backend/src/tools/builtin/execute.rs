use crate::tools::builtin::resolve_path;
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 execute_command 工具（PTY 命令执行）
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
            name: "execute_command".to_string(),
            description: "Execute any shell command in the workspace directory. Pass only the raw command (e.g. 'dir', 'ls -la', 'npm run build'), do NOT wrap it in powershell/cmd/bash/shell invocation - the tool handles that automatically. Params: command (required), description (optional), timeout (optional, default 60s), workdir (optional)"
                .to_string(),
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
                        &[], // 从缓存/配置读取黑名单
                    );

                    if result.blocked {
                        return Err(result.stdout);
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
}
