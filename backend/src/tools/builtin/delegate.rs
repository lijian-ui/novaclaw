use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 delegate_task 工具（Orchestrator 委托子 Agent）
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
            name: "delegate_task".to_string(),
            description: "Delegate a subtask to a specialized sub-agent. The sub-agent will think independently and complete the task, then report back.\nUse cases:\n- Need code review → delegate to code-reviewer\n- Need data analysis → delegate to data-analyst\n- Need web search → delegate to web-researcher\n\nYou can delegate multiple different tasks to different agents simultaneously. They will execute in parallel without blocking each other. For example:\n  delegate_task(\"code-reviewer\", \"Analyze project A\")\n  delegate_task(\"code-reviewer\", \"Analyze project B\")\n\nSub-agents can use their own tools to complete assigned tasks."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID, e.g. code-reviewer, data-analyst, web-researcher"
                    },
                    "task": {
                        "type": "string",
                        "description": "The specific task description to delegate to the agent"
                    }
                },
                "required": ["agent_id", "task"]
            }),
            handler: std::sync::Arc::new(
                |args: serde_json::Value,
                 chunk_tx: Option<
                    tokio::sync::mpsc::UnboundedSender<String>,
                >| -> Result<String, String> {
                    let agent_id = args["agent_id"]
                        .as_str()
                        .ok_or("Missing 'agent_id' parameter")?
                        .to_string();
                    let task = args["task"]
                        .as_str()
                        .ok_or("Missing 'task' parameter")?
                        .to_string();

                    // 发送子 Agent 启动事件
                    if let Some(ref tx) = chunk_tx {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "subagent", "action": "start",
                                "agent_id": agent_id, "task": task,
                            })
                            .to_string(),
                        );
                    }

                    // 在新的线程 + tokio runtime 中运行异步子 Agent
                    let rt = tokio::runtime::Runtime::new()
                        .map_err(|e| format!("Failed to create runtime: {}", e))?;

                    // 从文件系统加载 Agent 配置
                    let paths = crate::soul::SoulPaths::default();
                    let agent_config = crate::soul::AgentConfig::load(&paths, &agent_id)
                        .map_err(|e| format!("未找到智能体 '{}': {}", agent_id, e))?;
                    let soul_content = crate::soul::AgentConfig::get_soul_content(&paths, &agent_id)
                        .map_err(|e| format!("读取智能体 '{}' SOUL.md 失败: {}", agent_id, e))?;

                    tracing::info!(
                        "[SubAgent] 委托任务给 '{}' ({}): {}",
                        agent_config.id,
                        agent_config.name,
                        task
                    );

                    let result = rt.block_on(async {
                        let state = crate::APP_STATE.read().await;

                        // 确定使用的模型
                        let model_to_use = match agent_config.model.clone() {
                            Some(m) => m,
                            None => state.models_config.default_model.clone(),
                        };

                        // 获取 provider 和 config
                        let (provider, config, full_registry) = {
                            let provider = state
                                .models_config
                                .find_provider_by_model(&model_to_use)
                                .ok_or_else(|| {
                                    format!("未找到模型 '{}' 的提供商配置", model_to_use)
                                })?
                                .clone();
                            (provider, state.config.clone(), state.tool_registry.clone())
                        };

                        let llm_client =
                            crate::llm::client::LlmClient::new(provider, config.llm_timeout)
                                .map_err(|e| format!("创建 LLM 客户端失败: {}", e))?;

                        let sub_tools = if agent_config.enabled_tools.is_empty() {
                            full_registry
                        } else {
                            full_registry.filter_by_names(&agent_config.enabled_tools).await
                        };

                        let mut sub_session = crate::agent::session::AgentSession::new(
                            &agent_config.name,
                            &model_to_use,
                            None,
                        );
                        sub_session.system_prompt_override = Some(soul_content);
                        sub_session.push_user(&task);

                        let mut sub_config = config.clone();
                        sub_config.max_iterations = agent_config.max_iterations as usize;
                        if let Some(t) = agent_config.temperature {
                            sub_config.temperature = t;
                        }
                        if let Some(c) = agent_config.compact_threshold {
                            sub_config.compact_threshold = c;
                        }
                        if let Some(c) = agent_config.compact_keep {
                            sub_config.compact_keep = c;
                        }

                        let mut agent = crate::agent::runtime::AgentRuntime::new(
                            sub_session,
                            llm_client,
                            std::sync::Arc::new(sub_tools),
                            &sub_config,
                            vec![],
                        );

                        match agent.run_turn("", None, None, &[]).await {
                            Ok(result) => {
                                tracing::info!(
                                    "[SubAgent] '{}' 任务完成，输出 {} 字符",
                                    agent_config.name,
                                    result.content.len()
                                );
                                if let Some(ref tx) = chunk_tx {
                                    let _ = tx.send(
                                        serde_json::json!({
                                            "type": "subagent", "action": "done",
                                            "agent_id": agent_config.id, "name": agent_config.name,
                                            "result_length": result.content.len(),
                                        })
                                        .to_string(),
                                    );
                                }
                                Ok(format!(
                                    "## {} 执行结果\n\n{}\n\n---\n*员工: {} | 迭代: {} | 输入Token: {} | 输出Token: {}*",
                                    agent_config.name,
                                    result.content,
                                    agent_config.name,
                                    result.iterations,
                                    result.total_input_tokens,
                                    result.total_output_tokens,
                                ))
                            }
                            Err(e) => {
                                tracing::warn!("[SubAgent] '{}' 执行失败: {}", agent_config.name, e);
                                Err(format!("员工 '{}' 执行失败: {}", agent_config.name, e))
                            }
                        }
                    });

                    // 如果执行失败，也发送 done 事件标注失败
                    if result.is_err() {
                        if let Some(ref tx) = chunk_tx {
                            let _ = tx.send(
                                serde_json::json!({
                                    "type": "subagent", "action": "done",
                                    "agent_id": agent_id, "name": null,
                                    "error": true,
                                })
                                .to_string(),
                            );
                        }
                    }

                    result
                },
            ),
        })
        .await;
}
