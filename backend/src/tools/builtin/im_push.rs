use crate::im::types::*;
use crate::IM_GATEWAY;
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 注册 im_push 工具
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
                        name: "im_push".to_string(),
            display_name: "IM推送".to_string(),
            description: r#"Send a message to an IM platform (DingTalk, etc.) via a specific bot account.
Use this to proactively push notifications, alerts, or scheduled messages to users or groups.

The 'robot' specifies the bot account to send from (e.g. 'bot1', 'default').
The 'target_id' is the recipient — for private messages it's the user's userId,
for group messages it's the openConversationId.
You can find these IDs in the message context when users chat with the bot.

Examples:
- im_push(robot="bot1", target_type="private", target_id="manager123", content="任务完成")
- im_push(robot="bot2", target_type="group", target_id="cidxxx", content="大家好")"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "robot": {
                        "type": "string",
                        "description": "Bot account ID to send from (e.g. 'bot1', 'bot2', 'default')"
                    },
                    "target_type": {
                        "type": "string",
                        "enum": ["private", "group"],
                        "description": "Send to a private user or a group chat"
                    },
                    "target_id": {
                        "type": "string",
                        "description": "For private: user's userId. For group: openConversationId."
                    },
                    "content": {
                        "type": "string",
                        "description": "Message content (text or markdown)"
                    },
                    "content_type": {
                        "type": "string",
                        "enum": ["text", "markdown"],
                        "description": "Message format type (default: text)"
                    },
                    "title": {
                        "type": "string",
                        "description": "Title for markdown messages (required when content_type=markdown)"
                    }
                },
                "required": ["robot", "target_type", "target_id", "content"]
            }),
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>|
                 -> Result<String, String> {
                    let robot = args["robot"].as_str().ok_or("Missing 'robot' — specify which bot account to send from")?.to_string();
                    let target_type = args["target_type"].as_str().ok_or("Missing 'target_type'")?;
                    let target_id = args["target_id"].as_str().ok_or("Missing 'target_id'")?;
                    let content = args["content"].as_str().ok_or("Missing 'content'")?;
                    let content_type = args["content_type"].as_str().unwrap_or("text");
                    let title = args["title"].as_str().unwrap_or("");

                    let conversation_type = match target_type {
                        "private" => ConversationType::Private,
                        "group" => ConversationType::Group,
                        _ => return Err(format!("Invalid target_type: {}. Use 'private' or 'group'.", target_type)),
                    };

                    let target = MessageTarget {
                        account_id: robot.clone(),
                        platform: PlatformType::DingTalk,
                        conversation_id: target_id.to_string(),
                        conversation_type,
                    };

                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let gateway = IM_GATEWAY.read().await;
                        let gw = gateway.as_ref().ok_or("IM 网关未初始化")?;

                        // 使用 platform:account_id 复合 key 查找
                        let composite_key = format!("{}:{}", target.platform.as_str(), robot);
                        let adapter = gw.registry.get(&composite_key).await
                            .ok_or_else(|| format!("机器人账号 '{}' 未注册或未连接 (key={})", robot, composite_key))?;

                        let result = if content_type == "markdown" && !title.is_empty() {
                            adapter.send_markdown(&target, title, content).await
                        } else {
                            adapter.send_text(&target, content).await
                        };

                        match result {
                            Ok(_) => Ok(json!({"success": true, "message": format!("消息已通过 {} 发送", robot)}).to_string()),
                            Err(e) => Err(format!("发送失败: {}", e)),
                        }
                    })
                },
            ),
        }).await;
}
