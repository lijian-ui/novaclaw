//! 跨平台 IM 会话管理
//!
//! 管理 IM 消息到 Agent 会话的映射，支持跨平台会话持久化。
//! 参考 Hermes Agent 的 SessionSource 设计。

use crate::agent::session::{AgentMessage, AgentSession};
use crate::error::AppError;
use crate::im::types::{ConversationType, IncomingMessage, PlatformType, SessionSource};
use crate::storage::SessionStore;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// 从入站消息提取会话来源
pub fn session_source_from_incoming(msg: &IncomingMessage) -> SessionSource {
    SessionSource {
        account_id: msg.account_id.clone(),
        platform: msg.platform.clone(),
        conversation_id: msg.conversation_id.clone(),
        sender_id: msg.sender_id.clone(),
    }
}

/// 生成平台中文名称
pub fn platform_chinese_name(platform: &PlatformType) -> String {
    match platform {
        PlatformType::DingTalk => "钉钉".to_string(),
        PlatformType::WeChatWork => "企业微信".to_string(),
        PlatformType::Feishu => "飞书".to_string(),
        PlatformType::Slack => "Slack".to_string(),
        PlatformType::Discord => "Discord".to_string(),
        PlatformType::Telegram => "Telegram".to_string(),
        PlatformType::Custom(s) if s == "weixin" => "个人微信".to_string(),
        PlatformType::Custom(s) => format!("自定义({})", s),
    }
}

/// 格式化 IM 消息为 Agent 可读文本（注入平台上下文）
pub fn format_im_message(msg: &IncomingMessage) -> String {
    let platform_name = platform_chinese_name(&msg.platform);
    let conv_type = match msg.conversation_type {
        ConversationType::Private => "私聊",
        ConversationType::Group => "群聊",
    };
    let sender = msg.sender_name.as_deref().unwrap_or("未知用户");
    let group_name = msg.conversation_title.as_deref().unwrap_or("");
    let sender_id = msg.sender_id.as_deref().unwrap_or("");
    let sender_staff_id = msg.sender_staff_id.as_deref().unwrap_or("");
    let uid = if !sender_staff_id.is_empty() { sender_staff_id } else { sender_id };

    let prefix = match msg.conversation_type {
        ConversationType::Private => {
            format!(
                "[来自 {} {}，用户：{} (userId={}, robot={})]\n",
                platform_name, conv_type, sender, uid, msg.account_id
            )
        }
        ConversationType::Group => {
            if group_name.is_empty() {
                format!(
                    "[来自 {} {}，发送者：{} (userId={}, robot={})]\n",
                    platform_name, conv_type, sender, uid, msg.account_id
                )
            } else {
                format!(
                    "[来自 {} {}「{}」，发送者：{} (userId={}, robot={})]\n",
                    platform_name, conv_type, group_name, sender, uid, msg.account_id
                )
            }
        }
    };

    // 附加提示：告知 LLM 可以用 im_push 回复
    let mut result = format!("{}{}", prefix, msg.text);
    let target_id = match msg.conversation_type {
        ConversationType::Private => uid,
        ConversationType::Group => &msg.conversation_id,
    };
    let target_type_str = match msg.conversation_type {
        ConversationType::Private => "private",
        ConversationType::Group => "group",
    };
    if !target_id.is_empty() {
        let scene_desc = match msg.conversation_type {
            ConversationType::Private => {
                format!(
                    "\n\n[当前对话场景：{} {}，robot=\"{}\", target_id=\"{}\"]",
                    platform_name, conv_type, msg.account_id, target_id
                )
            }
            ConversationType::Group => {
                format!(
                    "\n\n[当前对话场景：{} {}，robot=\"{}\", openConversationId=\"{}\"]",
                    platform_name, conv_type, msg.account_id, target_id
                )
            }
        };
        result.push_str(&scene_desc);
    }

    result
}

/// IM 会话映射管理器
pub struct IMSessionManager {
    /// 会话来源 → Agent session ID 映射
    mapping: RwLock<HashMap<SessionSource, String>>,
    /// 会话互斥锁：防止同一会话并发处理导致状态冲突
    locks: RwLock<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// 会话持久化存储
    session_store: SessionStore,
}

use std::sync::Arc;

impl IMSessionManager {
    pub fn new(session_store: SessionStore) -> Self {
        Self {
            mapping: RwLock::new(HashMap::new()),
            locks: RwLock::new(HashMap::new()),
            session_store,
        }
    }

    /// 获取会话的互斥锁（用于并发控制）
    pub async fn get_session_lock(&self, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.write().await;
        locks.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }


    /// 获取或创建会话（使用确定性 session_id，重启后仍能恢复同一会话）
    pub async fn get_or_create(
        &self,
        source: &SessionSource,
        msg: &IncomingMessage,
    ) -> Result<AgentSession, AppError> {
        // 每次从 APP_STATE 读取最新的默认模型（支持前端切换后即时生效）
        let current_default_model = {
            let state = crate::APP_STATE.read().await;
            state.models_config.default_model.clone()
        };

        // 生成确定性 session_id：基于 account_id + platform + conversation_id 的哈希
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{}_{}_{}", source.account_id, source.platform, source.conversation_id).hash(&mut hasher);
        let sid = format!("im_{:x}", hasher.finish());

        // 检查映射（内存缓存）
        let cached = {
            let map = self.mapping.read().await;
            map.get(source).cloned()
        };

        // 尝试恢复已有会话（先查缓存，再查磁盘）
        let existing_session = cached.and_then(|id| self.session_store.get_session(&id).ok())
            .or_else(|| self.session_store.get_session(&sid).ok());

        if let Some(existing) = existing_session {
            let history = self.session_store.get_messages(&existing.id).unwrap_or_default();
            let mut session = AgentSession::new(&existing.name, &existing.model, None);
            session.id = existing.id.clone();
            for m in &history {
                session.push_message(AgentMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    first_reasoning: None,
                    again_reasonings: None,
                    reasoning: None,
                    images: None,
                    weight: 0,
                });

            }
            // 刷新缓存
            {
                let mut map = self.mapping.write().await;
                map.insert(source.clone(), session.id.clone());
            }
            tracing::debug!("恢复 IM 会话: {} ({})", session.id, source);
            return Ok(session);
        }

        // 没有已有会话，创建新的
        let bot_label = if let Some(ref name) = msg.account_name {
            format!("({})", name)
        } else {
            format!("({})", msg.account_id)
        };
        let conv_type_label = match msg.conversation_type {
            ConversationType::Private => format!("私聊{}", bot_label),
            ConversationType::Group => {
                if let Some(title) = &msg.conversation_title {
                    format!("群聊「{}」", title)
                } else {
                    "群聊".to_string()
                }
            }
        };
        let session_name = format!(
            "IM {} {}",
            platform_chinese_name(&source.platform),
            conv_type_label,
        );

        // 使用确定性 ID 创建（确保重启后同一来源恢复同一会话）
        let stored = self.session_store.create_session_with_id(
            &session_name,
            Some(&current_default_model),
            &sid,
        )?;

        let mut session = AgentSession::new(&session_name, &current_default_model, None);
        session.id = stored.id;

        // 存入内存映射
        {
            let mut map = self.mapping.write().await;
            map.insert(source.clone(), session.id.clone());
        }

        tracing::info!("创建新 IM 会话: {} (平台={})", session.id, source.platform);
        Ok(session)
    }
}

/// 检查群聊消息是否需要响应
pub fn should_respond_in_group(msg: &IncomingMessage) -> bool {
    if msg.conversation_type == ConversationType::Private {
        return true;
    }
    // 群聊：检查 raw 中的 @ 信息
    if let Some(obj) = msg.raw.as_object() {
        if let Some(is_at) = obj.get("isInAtList") {
            return is_at.as_bool().unwrap_or(true);
        }
        if let Some(at_users) = obj.get("atUsers") {
            if let Some(arr) = at_users.as_array() {
                return !arr.is_empty();
            }
        }
    }
    true // 无 @ 信息时默认响应
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::types::ConversationType;

    fn make_msg(
        text: &str,
        conv_type: ConversationType,
        sender: Option<&str>,
        group_title: Option<&str>,
    ) -> IncomingMessage {
        IncomingMessage {
            id: "1".into(),
            account_id: "default".into(),
            account_name: None,
            platform: PlatformType::DingTalk,
            conversation_id: "cid".into(),
            sender_id: sender.map(|s| s.into()),
            sender_name: sender.map(|s| s.into()),
            text: text.into(),
            media_urls: vec![],
            raw: serde_json::Value::Null,
            session_webhook: None,
            conversation_type: conv_type,
            conversation_title: group_title.map(|s| s.into()),
            timestamp: 1000,
        }
    }

    #[test]
    fn test_format_private() {
        let msg = make_msg("帮我查天气", ConversationType::Private, Some("张三"), None);
        let f = format_im_message(&msg);
        assert!(f.contains("钉钉") && f.contains("私聊") && f.contains("张三") && f.contains("帮我查天气"));
    }

    #[test]
    fn test_format_group() {
        let msg = make_msg("@机器人 你好", ConversationType::Group, Some("李四"), Some("项目群"));
        let f = format_im_message(&msg);
        assert!(f.contains("钉钉") && f.contains("群聊") && f.contains("项目群") && f.contains("李四"));
    }

    #[test]
    fn test_should_respond_private() {
        let msg = make_msg("hi", ConversationType::Private, None, None);
        assert!(should_respond_in_group(&msg));
    }
}
