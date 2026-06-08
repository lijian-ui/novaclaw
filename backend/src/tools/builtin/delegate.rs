use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 delegate_task 工具（Orchestrator 委托子 Agent）
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
                        name: "delegate_task".to_string(),
            display_name: "委托任务".to_string(),
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
            skip_truncation_save: false,
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

                    let workspace = args["_workspace"].as_str().map(|s| s.to_string());

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
                        "[SubAgent] 委托任务给 '{}' ({}): {} | 工作目录: {:?}",
                        agent_config.id,
                        agent_config.name,
                        task,
                        workspace
                    );

                    let result = rt.block_on(async {
                        // 尽量缩短持有锁的时间
                        let (model_to_use, provider, config, full_registry, models_config, subagent_soul) = {
                            let state = crate::APP_STATE.read().await;
                            let model = match agent_config.model.clone() {
                                Some(m) => m,
                                None => state.models_config.default_model.clone(),
                            };
                            let provider = state
                                .models_config
                                .find_provider_by_model(&model)
                                .ok_or_else(|| {
                                    format!("未找到模型 '{}' 的提供商配置", model)
                                })?
                                .clone();
                            let models_config = state.models_config.clone();

                            // 生成专用灵魂
                            let subagent_soul = crate::agent::prompt::SystemPromptBuilder::new(
                                &state.config,
                                "Unknown",
                                workspace.as_deref()
                            ).build_subagent_prompt(&agent_id, &task);

                            (model, provider, state.config.clone(), state.tool_registry.clone(), models_config, subagent_soul)
                        };

                        let llm_client =
                            crate::llm::client::LlmClient::new(provider, config.llm_timeout)
                                .map_err(|e| format!("创建 LLM 客户端失败: {}", e))?;

                        // 强力限制子 Agent 工具集：禁止写操作，强制搜索操作
                        let sub_tools = {
                            let mut tools_to_enable = if agent_config.enabled_tools.is_empty() {
                                vec!["read_file".to_string(), "list_dir".to_string(), "grep".to_string(), "glob".to_string()]
                            } else {
                                agent_config.enabled_tools.clone()
                            };

                            // 强制添加分析类工具
                            for core_tool in &["grep", "list_dir", "read_file", "glob", "pin_file"] {
                                if !tools_to_enable.contains(&core_tool.to_string()) {
                                    tools_to_enable.push(core_tool.to_string());
                                }
                            }

                            // 强制移除写和执行工具（除非是 code-reviewer 显式需要执行测试）
                            if agent_id == "code-explorer" || agent_id == "web-researcher" {
                                tools_to_enable.retain(|t| t != "write_file" && t != "execute_command" && t != "apply_patch");
                            }

                            full_registry.filter_by_names(&tools_to_enable).await
                        };

                        let mut sub_session = crate::agent::session::AgentSession::new(
                            &agent_config.name,
                            &model_to_use,
                            workspace.as_deref(),
                        );
                        sub_session.system_prompt_override = Some(subagent_soul);
                        sub_session.push_user(&task);

                        let mut sub_config = config.clone();
                        // 强力限制步数：子 Agent 不允许超过 15 步
                        sub_config.max_iterations = 15; 
                        
                        let mut agent = crate::agent::runtime::AgentRuntime::new(
                            sub_session,
                            llm_client,
                            std::sync::Arc::new(sub_tools),
                            &sub_config,
                            models_config,
                            vec![],
                        );

                        match agent.run_turn("", None, None, &[], &[]).await {
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
