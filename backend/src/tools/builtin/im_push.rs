use crate::im::adapter::MessageOptions;
use crate::im::types::*;
use crate::IM_GATEWAY;
use crate::tools::registry::{ToolDef, ToolRegistry};
use serde_json::json;

/// 将平台字符串解析为 PlatformType
fn parse_platform(s: &str) -> PlatformType {
    match s {
        "dingtalk" => PlatformType::DingTalk,
        "weixin" | "wechat" => PlatformType::Custom("weixin".to_string()),
        "wecom" | "wechatwork" => PlatformType::WeChatWork,
        "feishu" => PlatformType::Feishu,
        "slack" => PlatformType::Slack,
        "discord" => PlatformType::Discord,
        "telegram" => PlatformType::Telegram,
        other => PlatformType::Custom(other.to_string()),
    }
}

/// 注册 im_push 工具
pub async fn register(registry: &ToolRegistry) {
    registry
        .register(ToolDef {
                        name: "im_push".to_string(),
            display_name: "IM推送".to_string(),
            description: r#"Send a message to an IM platform via a specific bot account.
Use this to proactively push notifications, alerts, or scheduled messages to users or groups.

The 'platform' specifies which IM platform (e.g. 'dingtalk', 'weixin').
The 'robot' specifies the bot account to send from (e.g. 'bot1', 'default').
The 'target_id' is the recipient — for private messages it's the user's userId,
for group messages it's the openConversationId.
You can find these IDs in the message context when users chat with the bot.

Examples:
- im_push(platform="dingtalk", robot="bot1", target_type="private", target_id="manager123", content="任务完成")
- im_push(platform="weixin", robot="bot1", target_type="private", target_id="wx_user123", content="你好")"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "platform": {
                        "type": "string",
                        "description": "IM platform (e.g. 'dingtalk', 'weixin', 'wecom', 'feishu')",
                        "default": "dingtalk"
                    },
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
                    },
                    "image_url": {
                        "type": "string",
                        "description": "Image URL (remote http/https or local file path) to send as image message. When set, content/content_type/title are ignored."
                    },
                    "file_url": {
                        "type": "string",
                        "description": "File URL (remote http/https or local file path) to send as file attachment. Requires 'file_name'."
                    },
                    "file_name": {
                        "type": "string",
                        "description": "File name for file_url (e.g. 'report.pdf')"
                    },
                    "video_url": {
                        "type": "string",
                        "description": "Video URL (remote http/https or local file path) to send as video message."
                    }
                },
                "required": ["robot", "target_type", "target_id"]
            }),
            skip_truncation_save: false,
            handler: std::sync::Arc::new(
                move |args: serde_json::Value,
                      _chunk_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>|
                 -> Result<String, String> {
                    let platform_str = args["platform"].as_str().unwrap_or("dingtalk");
                    let robot = args["robot"].as_str().ok_or("Missing 'robot' — specify which bot account to send from")?.to_string();
                    let target_type = args["target_type"].as_str().ok_or("Missing 'target_type'")?;
                    let target_id = args["target_id"].as_str().ok_or("Missing 'target_id'")?;
                    let image_url = args.get("image_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    let file_url = args.get("file_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    let file_name = args.get("file_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    let video_url = args.get("video_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

                    let platform = parse_platform(platform_str);
                    let conversation_type = match target_type {
                        "private" => ConversationType::Private,
                        "group" => ConversationType::Group,
                        _ => return Err(format!("Invalid target_type: {}. Use 'private' or 'group'.", target_type)),
                    };

                    let target = MessageTarget {
                        account_id: robot.clone(),
                        platform,
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

                        // 优先级: image_url > file_url > video_url > markdown > text
                        let options = MessageOptions::default();
                        let result = if let Some(img_url) = image_url {
                            adapter.send_image(&target, img_url, None, &options).await
                        } else if let Some(f_url) = file_url {
                            let fname = file_name.unwrap_or("file");
                            adapter.send_file(&target, f_url, fname).await
                        } else if let Some(v_url) = video_url {
                            adapter.send_video(&target, v_url, None).await
                        } else {
                            let content = args["content"].as_str().ok_or("Missing 'content'")?;
                            let content_type = args["content_type"].as_str().unwrap_or("text");
                            let title = args["title"].as_str().unwrap_or("");
                            // 微信不支持 markdown
                            let supports_md = target.platform.as_str() == "dingtalk";
                            if content_type == "markdown" && !title.is_empty() && supports_md {
                                adapter.send_markdown(&target, title, content, &options).await
                            } else {
                                adapter.send_text(&target, content, &options).await
                            }
                        };

                        match result {
                            Ok(_) => Ok(json!({"success": true, "message": format!("消息已通过 {}:{} 发送", platform_str, robot)}).to_string()),
                            Err(e) => Err(format!("发送失败: {}", e)),
                        }
                    })
                },
            ),
        }).await;
}
