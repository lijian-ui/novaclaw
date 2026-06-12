use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::AppError;
use uuid::Uuid;

/// 会话数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

/// 工具调用参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallArgument {
    pub name: String,
    pub value: String,
}

/// 工具调用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// 消息数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    /// 工具调用列表（assistant 消息可包含）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 工具调用ID（tool 消息用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 工具名称（tool 消息用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// 第一次思考内容（CoT）- 用于前端显示为"思考过程"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_reasoning: Option<String>,
    /// 后续思考内容数组（CoT）- 用于前端显示为"Thought"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub again_reasonings: Option<Vec<String>>,
    /// 兼容旧字段：完整的推理内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Token 用量（仅 assistant 消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// 缓存 Token 用量（仅 assistant 消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    /// 最后一次请求的输入 Token（"本次输入"，仅 assistant 消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_input_tokens: Option<u64>,
    /// 最后一次请求的输出 Token（"本次输出"，仅 assistant 消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_output_tokens: Option<u64>,
    /// 缓存命中率（0.0 ~ 1.0，仅 assistant 消息，来自 session.cache_hit_rate()）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit_rate: Option<f64>,
    /// 图片引用路径列表（仅 user 消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_paths: Option<Vec<String>>,
    /// 消息类型标记（"compaction" 或 None 普通消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
}

/// 消息缓存上限：最多缓存 20 个活跃会话的消息，超过时淘汰最久未访问的
const CACHE_MAX_SESSIONS: usize = 20;

/// 会话存储管理
#[derive(Debug)]
pub struct SessionStore {
    sessions_dir: PathBuf,
    messages_dir: PathBuf,
    /// 消息内存缓存（LRU 淘汰）：避免每次对话轮次都完整读盘
    messages_cache: Mutex<MessageCache>,
}

/// 带 LRU 淘汰的消息缓存
#[derive(Debug)]
struct MessageCache {
    /// session_id → 消息列表
    map: HashMap<String, Vec<Message>>,
    /// LRU 顺序队列：队首=最近访问，队尾=最早访问（淘汰候选）
    lru: VecDeque<String>,
}

impl MessageCache {
    fn new() -> Self {
        Self { map: HashMap::new(), lru: VecDeque::new() }
    }

    fn get(&mut self, session_id: &str) -> Option<&Vec<Message>> {
        let hit = self.map.contains_key(session_id);
        if hit {
            // 移到队首（最近访问）
            if let Some(pos) = self.lru.iter().position(|id| id == session_id) {
                let id = self.lru.remove(pos).unwrap();
                self.lru.push_front(id);
            }
        }
        self.map.get(session_id)
    }

    fn insert(&mut self, session_id: String, messages: Vec<Message>) {
        // 如果已存在，先移除旧记录
        if self.map.contains_key(&session_id) {
            if let Some(pos) = self.lru.iter().position(|id| *id == session_id) {
                self.lru.remove(pos);
            }
        }

        // 淘汰：超过上限时移除最久未访问的（队尾）
        while self.map.len() >= CACHE_MAX_SESSIONS {
            if let Some(evict_id) = self.lru.pop_back() {
                self.map.remove(&evict_id);
            } else {
                break;
            }
        }

        self.map.insert(session_id.clone(), messages);
        self.lru.push_front(session_id);
    }

    fn remove(&mut self, session_id: &str) {
        self.map.remove(session_id);
        if let Some(pos) = self.lru.iter().position(|id| id == session_id) {
            self.lru.remove(pos);
        }
    }
}

// 手动实现 Clone
impl Clone for SessionStore {
    fn clone(&self) -> Self {
        Self {
            sessions_dir: self.sessions_dir.clone(),
            messages_dir: self.messages_dir.clone(),
            messages_cache: Mutex::new(MessageCache::new()),
        }
    }
}

impl SessionStore {
    /// 创建新的会话存储
    pub fn new(base_dir: &Path) -> Self {
        let sessions_dir = base_dir.to_path_buf();
        let messages_dir = base_dir.join("messages");

        fs::create_dir_all(&sessions_dir).ok();
        fs::create_dir_all(&messages_dir).ok();

        Self {
            sessions_dir,
            messages_dir,
            messages_cache: Mutex::new(MessageCache::new()),
        }
    }

    /// 列出所有会话
    pub fn list_sessions(&self) -> Result<Vec<Session>, AppError> {
        let mut sessions = Vec::new();

        if self.sessions_dir.exists() {
            for entry in fs::read_dir(&self.sessions_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file()
                    && path.extension().map_or(false, |ext| ext == "json")
                {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<Session>(&content) {
                            sessions.push(session);
                        }
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// 创建新会话（自动生成 UUID）
    pub fn create_session(&self, name: &str, model: Option<&str>) -> Result<Session, AppError> {
        self.create_session_with_id(name, model, &uuid::Uuid::new_v4().to_string())
    }

    /// 创建新会话（使用指定 ID，用于 IM 会话持久化映射）
    pub fn create_session_with_id(&self, name: &str, model: Option<&str>, id: &str) -> Result<Session, AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        let session = Session {
            id: id.to_string(),
            name: name.to_string(),
            created_at: now.clone(),
            updated_at: now,
            model: model.unwrap_or("").to_string(),
            metadata: None,
        };

        self.save_session_file(&session)?;
        tracing::info!("创建会话: {} ({})", session.name, session.id);
        Ok(session)
    }

    /// 获取指定会话
    pub fn get_session(&self, id: &str) -> Result<Session, AppError> {
        let path = self.session_path(id);
        if !path.exists() {
            return Err(AppError::NotFound(format!("会话不存在: {}", id)));
        }
        let content = fs::read_to_string(&path)?;
        let session = serde_json::from_str::<Session>(&content)?;
        Ok(session)
    }

    /// 删除会话及其消息
    /// 获取消息文件路径（调试用）
    pub fn messages_path_for_debug(&self, session_id: &str) -> PathBuf {
        self.messages_dir.join(format!("{}.jsonl", session_id))
    }

    pub fn delete_session(&self, id: &str) -> Result<(), AppError> {
        let session_path = self.session_path(id);
        if session_path.exists() {
            fs::remove_file(&session_path)?;
        }

        // 删除关联消息文件
        let msg_path = self.messages_path(id);
        if msg_path.exists() {
            fs::remove_file(&msg_path)?;
        }

        // 删除关联图片目录
        let images_path = self.messages_dir.parent()
            .map(|p| p.join("images").join(id))
            .unwrap_or_else(|| {
                let base = &self.messages_dir;
                let parent = base.parent().unwrap_or(base);
                parent.join("images").join(id)
            });
        if images_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&images_path) {
                tracing::warn!("删除图片目录失败 ({}): {}", images_path.display(), e);
                // 不阻塞主流程，仅记录警告
            }
        }

        // 删除关联媒体文件入站目录
        let media_inbound_path = crate::config::get_media_inbound_dir(id);
        if media_inbound_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&media_inbound_path) {
                tracing::warn!("删除媒体文件目录失败 ({}): {}", media_inbound_path.display(), e);
            }
        }

        // 清除内存缓存中的该会话
        {
            let mut cache = self.messages_cache.lock().unwrap();
            cache.remove(id);
        }

        tracing::info!("删除会话: {}", id);
        Ok(())
    }

    /// 更新会话
    pub fn update_session(&self, session: &Session) -> Result<(), AppError> {
        self.save_session_file(session)?;
        Ok(())
    }

    /// 获取会话消息列表（带 LRU 缓存，返回全部消息）
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<Message>, AppError> {
        // 1. 检查缓存（命中时自动更新 LRU 顺序）
        {
            let mut cache = self.messages_cache.lock().unwrap();
            if let Some(msgs) = cache.get(session_id) {
                return Ok(msgs.clone());
            }
        }

        // 2. 缓存未命中，从磁盘读取
        let path = self.messages_path(session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path)?;
        let messages: Vec<Message> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Message>(line).ok())
            .collect();

        // 3. 写入缓存（超过 CACHE_MAX_SESSIONS=20 时自动淘汰最久未访问的）
        {
            let mut cache = self.messages_cache.lock().unwrap();
            cache.insert(session_id.to_string(), messages.clone());
        }

        Ok(messages)
    }

    /// 获取 compaction 感知的消息列表（用于 Agent 上下文加载）
    /// 从后向前扫描，找到最后一条 compaction 标记，仅返回该标记之后的消息
    /// 参考 Codex: rollout_reconstruction.rs 反向扫描找到 Compacted 点
    pub fn get_messages_since_last_compaction(&self, session_id: &str) -> Result<Vec<Message>, AppError> {
        let all = self.get_messages(session_id)?;
        let last_compaction_idx = all.iter().rposition(|m| {
            m.message_type.as_deref() == Some("compaction")
        });
        if let Some(idx) = last_compaction_idx {
            // 跳过 compaction 标记本身和它之前的所有消息
            Ok(all[idx + 1..].to_vec())
        } else {
            Ok(all)
        }
    }

    /// 写入 compaction 标记到 JSONL（参考 Codex: 插入 Compacted 标记行）
    /// 此标记表示之前的历史已被 AI 摘要替代，加载时跳过旧内容
    pub fn write_compaction_marker(&self, session_id: &str, summary: &str) -> Result<(), AppError> {
        let marker = Message {
            id: format!("compaction_{}", uuid::Uuid::new_v4()),
            session_id: session_id.to_string(),
            role: "system".to_string(),
            content: summary.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: None,
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
            input_tokens: None,
            output_tokens: None,
            cached_tokens: None,
            last_input_tokens: None,
            last_output_tokens: None,
            cache_hit_rate: None,
            image_paths: None,
            message_type: Some("compaction".to_string()),
        };
        self.append_message(session_id, &marker)?;

        // 写完后清除缓存（下次 get_messages 会重新加载，自动应用 compaction 过滤）
        {
            let mut cache = self.messages_cache.lock().unwrap();
            cache.remove(session_id);
        }

        Ok(())
    }

    /// 追加消息（JSONL 格式增量写入 + 更新内存缓存）
    pub fn append_message(&self, session_id: &str, message: &Message) -> Result<(), AppError> {
        let path = self.messages_path(session_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let line = serde_json::to_string(message)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(file, "{}", line)?;
        file.flush()?;

        // 更新内存缓存（如果缓存中有该会话，直接在末尾追加）
        {
            let mut cache = self.messages_cache.lock().unwrap();
            if let Some(msgs) = cache.map.get_mut(session_id) {
                msgs.push(message.clone());
            }
        }

        // 更新会话时间戳
        let now = chrono::Utc::now().to_rfc3339();
        if let Ok(mut session) = self.get_session(session_id) {
            session.updated_at = now;
            self.save_session_file(&session)?;
        }

        Ok(())
    }

    // ---- 内部辅助方法 ----

    fn session_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.json", id))
    }

    fn messages_path(&self, session_id: &str) -> PathBuf {
        self.messages_dir.join(format!("{}.jsonl", session_id))
    }

    fn save_session_file(&self, session: &Session) -> Result<(), AppError> {
        let path = self.session_path(&session.id);
        let content = serde_json::to_string_pretty(session)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        Ok(())
    }
}
