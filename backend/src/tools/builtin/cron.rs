use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 cron 工具（定时任务管理）
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
            name: "cron".to_string(),
            description:
                "Manage cron jobs and scheduled tasks. You MUST write a valid 5-field cron expression yourself. Actions: list (list all), create (name + cron expression + payload), get (by id), update (by id), remove (by id), run (by id)"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "create", "get", "update", "remove", "run"],
                        "description": "Action to perform"
                    },
                    "name": {
                        "type": "string",
                        "description": "Job name (required for create)"
                    },
                    "schedule": {
                        "type": "string",
                        "description": "5-field cron expression (e.g. '0 8 * * *' for daily 8am). You MUST generate this — do NOT use natural language!"
                    },
                    "payload": {
                        "type": "string",
                        "description": "Task prompt content to execute when the job fires (required for create)"
                    },
                    "id": {
                        "type": "string",
                        "description": "Job ID (required for get/update/remove/run)"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "(Optional) Chat session ID for results delivery — if omitted, a dedicated cron session is created automatically"
                    }
                },
                "required": ["action"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 _chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("Missing 'action' parameter")?;
                    let store_arc = crate::cron::get_store();
                    let rt = tokio::runtime::Handle::current();

                    match action {
                        "list" => {
                            let guard = rt.block_on(async { store_arc.lock().await });
                            let jobs: Vec<crate::cron::CronJob> = guard.list().to_vec();
                            drop(guard);
                            let output: Vec<String> = jobs
                                .iter()
                                .map(|j| {
                                    format!(
                                        "[{}] {} | schedule: {} | enabled: {} | runs: {}",
                                        j.id, j.name, j.schedule, j.enabled, j.run_count
                                    )
                                })
                                .collect();
                            if output.is_empty() {
                                Ok("暂无定时任务".to_string())
                            } else {
                                Ok(output.join("\n"))
                            }
                        }
                        "get" => {
                            let id = args["id"].as_str().ok_or("Missing 'id' parameter")?;
                            let guard = rt.block_on(async { store_arc.lock().await });
                            let job = guard.get(id).cloned();
                            drop(guard);
                            match job {
                                Some(j) => Ok(format!(
                                    "ID: {}\n名称: {}\n调度: {}\n启用: {}\n状态: {}\n执行次数: {}\n上次执行: {:?}\n下次执行: {:?}\n负载: {}",
                                    j.id,
                                    j.name,
                                    j.schedule,
                                    j.enabled,
                                    j.status,
                                    j.run_count,
                                    j.last_run_at,
                                    j.next_run_at,
                                    j.payload
                                )),
                                None => Err(format!("定时任务 '{}' 未找到", id)),
                            }
                        }
                        "create" => {
                            let name = args["name"].as_str().ok_or("Missing 'name' parameter")?;
                            let schedule = args["schedule"].as_str().unwrap_or("0 * * * *");
                            let payload = args["payload"].as_str().unwrap_or("");

                            let id = uuid::Uuid::new_v4().to_string();
                            let now = chrono::Utc::now().to_rfc3339();
                            let next_run = crate::cron::compute_initial_next_run(schedule);

                            let cron_session_name = format!("⏰ {}", name);
                            let cron_session = rt
                                .block_on(async {
                                    crate::APP_STATE
                                        .read()
                                        .await
                                        .session_store
                                        .create_session(&cron_session_name, None)
                                        .map_err(|e| format!("创建定时任务会话失败: {}", e))
                                })?;

                            let job = crate::cron::CronJob {
                                id: id.clone(),
                                name: name.to_string(),
                                schedule: schedule.to_string(),
                                enabled: true,
                                payload: payload.to_string(),
                                session_id: Some(cron_session.id.clone()),
                                created_at: now.clone(),
                                updated_at: now,
                                last_run_at: None,
                                next_run_at: Some(next_run),
                                status: "idle".to_string(),
                                run_count: 0,
                                last_error: None,
                            };

                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            guard.add(job);
                            drop(guard);
                            Ok(format!("定时任务 '{}' 已创建 (ID: {})", name, id))
                        }
                        "update" => {
                            let id = args["id"]
                                .as_str()
                                .ok_or("Missing 'id' parameter")?
                                .to_string();
                            let name = args["name"].as_str().map(|s| s.to_string());
                            let schedule = args["schedule"].as_str().map(|s| s.to_string());
                            let payload = args["payload"].as_str().map(|s| s.to_string());
                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            let updated = guard.update(&id, |job| {
                                if let Some(ref n) = name {
                                    job.name = n.clone();
                                }
                                if let Some(ref sch) = schedule {
                                    job.schedule = sch.clone();
                                    job.next_run_at =
                                        Some(crate::cron::compute_initial_next_run(sch));
                                }
                                if let Some(ref p) = payload {
                                    job.payload = p.clone();
                                }
                            });
                            drop(guard);
                            if updated {
                                Ok(format!("定时任务已更新"))
                            } else {
                                Err(format!("定时任务 '{}' 未找到", id))
                            }
                        }
                        "remove" => {
                            let id = args["id"]
                                .as_str()
                                .ok_or("Missing 'id' parameter")?
                                .to_string();
                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            let removed = guard.remove(&id);
                            drop(guard);
                            if removed {
                                let _ = crate::logging::delete_task_log(&id);
                                Ok(format!("定时任务已删除"))
                            } else {
                                Err(format!("定时任务 '{}' 未找到", id))
                            }
                        }
                        "run" => {
                            let id = args["id"]
                                .as_str()
                                .ok_or("Missing 'id' parameter")?
                                .to_string();
                            let mut guard = rt.block_on(async { store_arc.lock().await });
                            let updated = guard.update(&id, |job| {
                                job.last_run_at = Some(chrono::Utc::now().to_rfc3339());
                                job.run_count += 1;
                                tracing::info!("[Cron] 手动触发任务: {}", job.name);
                            });
                            drop(guard);
                            if updated {
                                Ok(format!("任务已手动触发"))
                            } else {
                                Err(format!("定时任务 '{}' 未找到", id))
                            }
                        }
                        _ => Err(format!("未知操作: {}", action)),
                    }
                },
            ),
        })
        .await;
}
