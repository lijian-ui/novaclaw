use crate::soul::{AgentConfig, SoulPaths};
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 agent_manage 工具
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
            name: "agent_manage".to_string(),
            display_name: "智能体管理".to_string(),
            description: r#"Create, view, update, and delete AI agents (sub-agents).

Each agent has its own identity (SOUL.md), tool set, and model configuration.

Actions:
- "list" — List all agents with their id, name, description, tools, and has_soul status.
- "list_tools" — List all available tools that can be assigned to agents. Call this before create to see what tools exist.
- "create" — Create a new agent. Requires: id, name, description. Optional: enabled_tools, model, soul.
- "view" — View agent details including config and SOUL.md content. Requires: id.
- "update" — Update agent config (name, description, model, enabled_tools, etc). Requires: id.
- "delete" — Delete an agent. Requires: id (cannot delete 'default').
- "set_soul" — Set or update SOUL.md content. Requires: id, soul.

Examples:
- agent_manage(action="create", id="ppt-dev", name="PPT开发助手", description="专门开发PPT")
- agent_manage(action="list")
- agent_manage(action="set_soul", id="ppt-dev", soul="你是PPT开发专家...")"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "list_tools", "create", "view", "update", "delete", "set_soul"],
                        "description": "Action: list (agents), list_tools (available tools), create, view, update, delete, set_soul"
                    },
                    "id": {
                        "type": "string",
                        "description": "Agent ID (required for create/view/update/delete/set_soul)"
                    },
                    "name": {
                        "type": "string",
                        "description": "Agent display name (required for create)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Agent description (required for create)"
                    },
                    "model": {
                        "type": "string",
                        "description": "Model name (optional, uses default if omitted)"
                    },
                    "enabled_tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "IMPORTANT: Call action='list_tools' FIRST to see all available tool names. Use the actual tool names here (e.g. ['read_file', 'web_search']). If omitted or empty, ALL tools are available — only restrict when the agent genuinely shouldn't access certain tools."
                    },
                    "soul": {
                        "type": "string",
                        "description": "SOUL.md content — identity/personality instructions (required for set_soul)"
                    },
                    "max_iterations": {
                        "type": "integer",
                        "description": "Max iterations (0 = unlimited, optional)"
                    },
                    "temperature": {
                        "type": "number",
                        "description": "LLM temperature (optional, uses global default if omitted)"
                    }
                },
                "required": ["action", "soul"]
            }),
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>|
                 -> Result<String, String> {
                    let action = args["action"].as_str().ok_or("Missing 'action'")?;
                    let paths = SoulPaths::default();

                    match action {
                        "list" => {
                            let agent_names = AgentConfig::list_all(&paths);
                            let mut agents = Vec::new();
                            for name in agent_names {
                                match AgentConfig::load(&paths, &name) {
                                    Ok(config) => {
                                        let has_soul = std::path::Path::new(&paths.soul_path(&name)).exists();
                                        agents.push(json!({
                                            "id": config.id, "name": config.name,
                                            "description": config.description,
                                            "model": config.model,
                                            "enabled_tools": config.enabled_tools,
                                            "has_soul": has_soul,
                                        }));
                                    }
                                    Err(_) => {
                                        agents.push(json!({
                                            "id": name, "name": name, "description": "",
                                            "model": null, "enabled_tools": [],
                                            "has_soul": std::path::Path::new(&paths.soul_path(&name)).exists(),
                                        }));
                                    }
                                }
                            }
                            Ok(json!({"success": true, "data": agents}).to_string())
                        }

                        "list_tools" => {
                            let rt = tokio::runtime::Handle::current();
                            let tools = rt.block_on(async {
                                let state = crate::APP_STATE.read().await;
                                state.tool_registry.list_tools_info().await
                            });
                            Ok(json!({"success": true, "data": tools}).to_string())
                        }

                        "create" => {
                            let agent_id = args["id"].as_str().ok_or("Missing 'id' for create")?;
                            let name = args["name"].as_str().ok_or("Missing 'name' for create")?;
                            let description = args["description"].as_str().ok_or("Missing 'description' for create")?;

                            let existing = AgentConfig::list_all(&paths);
                            if existing.contains(&agent_id.to_string()) {
                                return Err(format!("Agent '{}' already exists. Use 'update' to modify.", agent_id));
                            }

                            let config = AgentConfig {
                                id: agent_id.to_string(),
                                name: name.to_string(),
                                description: description.to_string(),
                                model: args["model"].as_str().map(|s| s.to_string()),
                                enabled_tools: args["enabled_tools"].as_array()
                                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                    .unwrap_or_default(),
                                max_iterations: args["max_iterations"].as_u64().unwrap_or(0) as u32,
                                temperature: args["temperature"].as_f64(),
                                compact_threshold: None,
                                compact_keep: None,
                            };
                            config.save(&paths)?;

                            if let Some(soul) = args["soul"].as_str() {
                                if !soul.is_empty() {
                                    AgentConfig::save_soul_content(&paths, agent_id, soul)?;
                                }
                            }

                            Ok(json!({"success": true, "message": format!("Agent '{}' created", agent_id)}).to_string())
                        }

                        "view" => {
                            let agent_id = args["id"].as_str().ok_or("Missing 'id' for view")?;
                            let config = AgentConfig::load(&paths, agent_id)?;
                            let soul = AgentConfig::get_soul_content(&paths, agent_id).ok();
                            Ok(json!({
                                "success": true,
                                "data": {
                                    "id": config.id, "name": config.name,
                                    "description": config.description,
                                    "model": config.model,
                                    "enabled_tools": config.enabled_tools,
                                    "max_iterations": config.max_iterations,
                                    "temperature": config.temperature,
                                    "has_soul": soul.is_some(),
                                    "soul": soul,
                                }
                            }).to_string())
                        }

                        "update" => {
                            let agent_id = args["id"].as_str().ok_or("Missing 'id' for update")?;
                            let mut config = AgentConfig::load(&paths, agent_id)?;

                            if let Some(name) = args["name"].as_str() { config.name = name.to_string(); }
                            if let Some(desc) = args["description"].as_str() { config.description = desc.to_string(); }
                            if let Some(model) = args["model"].as_str() {
                                config.model = if model.is_empty() { None } else { Some(model.to_string()) };
                            }
                            if let Some(tools) = args["enabled_tools"].as_array() {
                                config.enabled_tools = tools.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                            }
                            if let Some(iter) = args["max_iterations"].as_u64() { config.max_iterations = iter as u32; }
                            if args.get("temperature").and_then(|v| v.as_f64()).is_some() {
                                config.temperature = args["temperature"].as_f64();
                            }
                            config.save(&paths)?;
                            Ok(json!({"success": true, "message": format!("Agent '{}' updated", agent_id)}).to_string())
                        }

                        "delete" => {
                            let agent_id = args["id"].as_str().ok_or("Missing 'id' for delete")?;
                            AgentConfig::remove(&paths, agent_id)?;
                            Ok(json!({"success": true, "message": format!("Agent '{}' deleted", agent_id)}).to_string())
                        }

                        "set_soul" => {
                            let agent_id = args["id"].as_str().ok_or("Missing 'id' for set_soul")?;
                            let soul = args["soul"].as_str().ok_or("Missing 'soul' content")?;
                            AgentConfig::save_soul_content(&paths, agent_id, soul)?;
                            Ok(json!({"success": true, "message": format!("SOUL.md set for agent '{}'", agent_id)}).to_string())
                        }

                        _ => Err(format!("Unknown action: {}. Use: list, create, view, update, delete, set_soul", action)),
                    }
                },
            ),
        }).await;
}
