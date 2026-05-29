use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::post,
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;

type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

use crate::agent::runtime::AgentRuntime;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::llm::types::ChatRequest;
use crate::storage;
use crate::tools::types::AgentStep;
use crate::APP_STATE;

#[derive(Deserialize)]
struct ChatRequestHttp {
    #[serde(default)]
    session_id: Option<String>,
    message: String,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct ChatStreamRequest {
    #[serde(default)]
    session_id: Option<String>,
    message: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    images: Vec<String>, // ["data:image/png;base64,iVBORw..."]
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Deserialize)]
struct TestConnectionReq {
    api_key: String,
    base_url: String,
    model: String,
}

fn make_session_title(msg: &str) -> String {
    let cleaned: String = msg.chars().take(50).collect();
    let cleaned = cleaned.replace('\r', " ").replace('\n', " ");
    let trimmed = cleaned.trim().to_string();
    if trimmed.is_empty() { "新对话".to_string() } else { trimmed }
}

/// 获取图片存储目录
fn get_images_dir() -> std::path::PathBuf {
    crate::config::get_sessions_dir().join("images")
}

/// 解码 data: URL → 保存到磁盘，返回相对文件名
fn save_image_data_url(data_url: &str, session_id: &str) -> Result<String, String> {
    // 解析 "data:image/png;base64,iVBORw..."
    let after_comma = data_url.find(',').ok_or("Invalid data URL format")?;
    let header = &data_url[..after_comma];   // "data:image/png;base64"
    let b64 = &data_url[after_comma + 1..];

    // 提取 MIME → 扩展名
    let mime = header
        .trim_start_matches("data:")
        .trim_end_matches(";base64");
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return Err(format!("Unsupported image type: {}", mime)),
    };

    // Base64 解码
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    // 写磁盘
    let dir = get_images_dir().join(session_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Create image dir failed: {}", e))?;
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let filepath = dir.join(&filename);
    std::fs::write(&filepath, &data).map_err(|e| format!("Write image failed: {}", e))?;

    Ok(filename)
}

fn storage_msg_to_agent_msg(m: &storage::Message) -> AgentMessage {
    let tool_calls = m.tool_calls.as_ref().map(|tcs| {
        tcs.iter().map(|tc| AgentToolCall {
            id: tc.id.clone(), name: tc.name.clone(), arguments: tc.arguments.clone().unwrap_or_default(),
        }).collect()
    });
    AgentMessage {
        role: m.role.clone(), content: m.content.clone(), tool_calls,
        tool_call_id: m.tool_call_id.clone(), tool_name: m.tool_name.clone(),
        first_reasoning: m.first_reasoning.clone(), again_reasonings: m.again_reasonings.clone(), reasoning: m.reasoning.clone(),
        images: None,
    }
}

fn make_storage_msg(session_id: &str, role: &str, content: &str, tool_calls: Option<Vec<storage::ToolCall>>, tool_call_id: Option<String>, tool_name: Option<String>) -> storage::Message {
    storage::Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: None, tool_calls, tool_call_id, tool_name,
        first_reasoning: None, again_reasonings: None, reasoning: None,
        input_tokens: None, output_tokens: None, cached_tokens: None,
        last_input_tokens: None, last_output_tokens: None,
        image_paths: None, message_type: None,
        cache_hit_rate: None,
    }
}

fn make_storage_msg_with_reasoning(session_id: &str, role: &str, content: &str, tool_calls: Option<Vec<storage::ToolCall>>, tool_call_id: Option<String>, tool_name: Option<String>, first_reasoning: Option<String>, again_reasonings: Option<Vec<String>>, reasoning: Option<String>) -> storage::Message {
    storage::Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: None, tool_calls, tool_call_id, tool_name,
        first_reasoning, again_reasonings, reasoning,
        input_tokens: None, output_tokens: None, cached_tokens: None,
        last_input_tokens: None, last_output_tokens: None,
        image_paths: None, message_type: None,
        cache_hit_rate: None,
    }
}

fn make_storage_msg_with_tokens(session_id: &str, role: &str, content: &str, tool_calls: Option<Vec<storage::ToolCall>>, tool_call_id: Option<String>, tool_name: Option<String>, first_reasoning: Option<String>, again_reasonings: Option<Vec<String>>, reasoning: Option<String>, input_tokens: u64, output_tokens: u64, cached_tokens: u64, last_input_tokens: u64, last_output_tokens: u64, cache_hit_rate: f64) -> storage::Message {
    storage::Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: None, tool_calls, tool_call_id, tool_name,
        first_reasoning, again_reasonings, reasoning,
        input_tokens: Some(input_tokens), output_tokens: Some(output_tokens), cached_tokens: Some(cached_tokens),
        last_input_tokens: Some(last_input_tokens), last_output_tokens: Some(last_output_tokens),
        cache_hit_rate: Some(cache_hit_rate),
        image_paths: None, message_type: None,
    }
}

async fn chat(Json(req): Json<ChatRequestHttp>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    let session_id = match &req.session_id {
        Some(id) => id.clone(),
        None => {
            let name = make_session_title(&req.message);
            match state.session_store.create_session(&name, req.model.as_deref()) {
                Ok(s) => s.id,
                Err(e) => return Json(serde_json::json!({ "success": false, "message": e.to_string() })),
            }
        }
    };
    let model = req.model.clone().unwrap_or_else(|| state.models_config.default_model.clone());
    let provider = match state.models_config.find_provider_by_model(&model) {
        Some(p) => p.clone(),
        None => return Json(serde_json::json!({"success": false, "message": format!("未找到模型 '{}' 的提供商配置", model)})),
    };
    let llm_client = match crate::llm::client::LlmClient::new(provider, state.config.llm_timeout) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"success": false, "message": e.to_string()})),
    };
    let history = state.session_store.get_messages(&session_id).unwrap_or_default();
    let mut agent_session = AgentSession::new(&make_session_title(&req.message), &model, None);
    agent_session.id = session_id.clone();
    for m in &history { if m.role != "system" { agent_session.push_message(storage_msg_to_agent_msg(m)); } }

    let mut runtime = AgentRuntime::new(agent_session, llm_client, Arc::new(state.tool_registry.clone()), &state.config, state.models_config.clone(), state.skills_loader.list_skills());
    let result = match runtime.run_turn(&req.message, None, None, &[]).await {
        Ok(r) => r, Err(e) => return Json(serde_json::json!({"success": false, "message": e.to_string()})),
    };

    let history_len = state.session_store.get_messages(&session_id).unwrap_or_default().len();
    let _ = state.session_store.append_message(&session_id, &make_storage_msg(&session_id, "user", &req.message, None, None, None));
    let new_msgs = if history_len <= result.messages.len() { &result.messages[history_len..] } else { &result.messages[..] };
    for m in new_msgs {
        // 用户消息已在上方保存，跳过避免重复
        if m.role == "user" { continue; }
        let tcs = m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| storage::ToolCall { id: tc.id.clone(), name: tc.name.clone(), arguments: Some(tc.arguments.clone()) }).collect());
        let _ = state.session_store.append_message(&session_id, &make_storage_msg_with_reasoning(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone(), m.first_reasoning.clone(), m.again_reasonings.clone(), m.reasoning.clone()));
    }

    Json(serde_json::json!({"success": true, "data": {"session_id": session_id, "message_id": uuid::Uuid::new_v4().to_string(), "content": result.content, "role": "assistant"}}))
}

async fn chat_stream(Json(req): Json<ChatStreamRequest>) -> Sse<SseEventStream> {
    let (sse_tx, sse_rx) = mpsc::channel::<String>(256);
    let (step_tx, mut step_rx) = mpsc::channel::<AgentStep>(32);

    tokio::spawn(async move {
        let state = APP_STATE.read().await;
        let session_id = match &req.session_id {
            Some(id) => id.clone(),
            None => {
                let name = make_session_title(&req.message);
                match state.session_store.create_session(&name, req.model.as_deref()) {
                    Ok(s) => s.id,
                    Err(e) => { let _ = sse_tx.send(serde_json::json!({"type": "error", "data": {"message": e.to_string()}}).to_string()).await; return; }
                }
            }
        };
        let model = req.model.clone().unwrap_or_else(|| state.models_config.default_model.clone());
        let provider = match state.models_config.find_provider_by_model(&model) {
            Some(p) => p.clone(),
            None => { let _ = sse_tx.send(serde_json::json!({"type": "error", "data": {"message": format!("未找到模型 '{}' 的提供商配置", model)}}).to_string()).await; return; }
        };
        let history = state.session_store.get_messages(&session_id).unwrap_or_default();
        let mut agent_session = AgentSession::new(&make_session_title(&req.message), &model, req.workspace.as_deref());
        agent_session.id = session_id.clone();
        // 如果指定了智能体，从文件系统加载 SOUL.md 和 Agent 配置
        let mut config = state.config.clone();
        if let Some(ref agent_id) = req.agent_id {
            let paths = crate::soul::SoulPaths::default();
            // 加载 SOUL.md
            match crate::soul::AgentConfig::get_soul_content(&paths, agent_id) {
                Ok(soul_content) => {
                    agent_session.system_prompt_override = Some(soul_content);
                    tracing::info!("[Agent] 用户选择智能体: id={}", agent_id);
                }
                Err(_) => {
                    if agent_id != "default" {
                        tracing::warn!("[Agent] 用户选择的智能体 '{}' 未找到 SOUL.md，使用默认提示词", agent_id);
                    }
                }
            }
            // 加载 agent.json 并合并温度/压缩配置
            if let Ok(agent_cfg) = crate::soul::AgentConfig::load(&paths, agent_id) {
                if let Some(t) = agent_cfg.temperature {
                    config.temperature = t;
                }
                if let Some(c) = agent_cfg.compact_threshold {
                    config.compact_threshold = c;
                }
                if let Some(c) = agent_cfg.compact_keep {
                    config.compact_keep = c;
                }
            }
        } else {
            tracing::debug!("[Agent] 使用默认智能体（未指定 agent_id）");
        }
        for m in &history { if m.role != "system" { agent_session.push_message(storage_msg_to_agent_msg(m)); } }
        // 从历史消息中获取累计 Token 计数（input_tokens 已存储为累计值，取最后一条即可）
        // 注意：累计值存放在最后一条带 input_tokens 的 assistant 消息中
        let cumulative_input_from_history: u64 = history.iter()
            .filter(|m| m.role == "assistant" && m.input_tokens.is_some())
            .last()
            .and_then(|m| m.input_tokens)
            .unwrap_or(0);
        let cumulative_output_from_history: u64 = history.iter()
            .filter(|m| m.role == "assistant" && m.output_tokens.is_some())
            .last()
            .and_then(|m| m.output_tokens)
            .unwrap_or(0);
        // 将历史累计值预填入 AgentSession，这样 runtime 内部的累加（self.session.total_input_tokens += ...）
        // 自动成为会话级累计值，无需 chat.rs 再手动做加法
        agent_session.total_input_tokens = cumulative_input_from_history;
        agent_session.total_output_tokens = cumulative_output_from_history;

        let llm_client = match crate::llm::client::LlmClient::new(provider, state.config.llm_timeout) {
            Ok(c) => c,
            Err(e) => { let _ = sse_tx.send(serde_json::json!({"type": "error", "data": {"message": e.to_string()}}).to_string()).await; return; }
        };
        let skills = state.skills_loader.list_skills();
        let tool_registry = Arc::new(state.tool_registry.clone());
        let history_msg_count = agent_session.messages.len();
        let models_config = state.models_config.clone();
        drop(state);

        // 创建取消标志并注册到 cancel_map
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut state = APP_STATE.write().await;
            state.cancel_map.insert(session_id.clone(), cancel_flag.clone());
        }

        let step_sse_tx = sse_tx.clone();
        let step_fwd_handle = tokio::spawn(async move {
            while let Some(step) = step_rx.recv().await {
                let event_json = match step.step_type.as_str() {
                    "text_chunk" => serde_json::json!({"type": "chunk", "data": step.content}),
                    "approval_required" => serde_json::json!({"type": "agent_step", "data": {
                        "step_type": step.step_type, "content": step.content, "tool_name": step.tool_name,
                        "tool_result": step.tool_result, "turn": step.turn, "max_turns": step.max_turns,
                        "approval": step.approval, "approval_id": step.approval_id,
                    }}),
                    _ => serde_json::json!({"type": "agent_step", "data": {
                        "step_type": step.step_type, "content": step.content, "tool_name": step.tool_name,
                        "tool_result": step.tool_result, "turn": step.turn, "max_turns": step.max_turns,
                    }}),
                };
                if step_sse_tx.send(event_json.to_string()).await.is_err() { break; }
            }
        });

        let agent_sse_tx = sse_tx.clone();
        tokio::spawn(async move {
            // 保存图片到磁盘，收集路径
            let mut saved_image_paths: Vec<String> = Vec::new();
            let mut image_data_urls: Vec<String> = Vec::new();
            for data_url in &req.images {
                match save_image_data_url(data_url, &session_id) {
                    Ok(filename) => {
                        saved_image_paths.push(filename);
                        image_data_urls.push(data_url.clone());
                    }
                    Err(e) => tracing::warn!("Image save failed: {}", e),
                }
            }

            let mut runtime = AgentRuntime::new(agent_session, llm_client, tool_registry, &config, models_config, skills);
            let result = runtime.run_turn(&req.message, Some(step_tx), Some(cancel_flag), &image_data_urls).await;
            let _ = step_fwd_handle.await;

            match result {
                Ok(agent_result) => {
                    let state = APP_STATE.read().await;
                    let mut user_msg = make_storage_msg(&session_id, "user", &req.message, None, None, None);
                    if !saved_image_paths.is_empty() {
                        user_msg.image_paths = Some(saved_image_paths.clone());
                    }
                    let _ = state.session_store.append_message(&session_id, &user_msg);
                    let new_msgs = if history_msg_count <= agent_result.messages.len() { &agent_result.messages[history_msg_count..] }
                        else { let keep = crate::agent::runtime::COMPACT_KEEP_LAST_FALLBACK; if agent_result.messages.len() > keep { &agent_result.messages[agent_result.messages.len() - keep..] } else { &agent_result.messages[..] } };
                    let msg_count = new_msgs.len();
                    for (i, m) in new_msgs.iter().enumerate() {
                        // 用户消息已在上方保存，跳过避免重复
                        if m.role == "user" { continue; }
                        let tcs = m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| storage::ToolCall { id: tc.id.clone(), name: tc.name.clone(), arguments: Some(tc.arguments.clone()) }).collect());
                        // 仅最后一条 assistant 消息携带 Token 用量
                        // input_tokens / output_tokens 已经是会话级累计值（runtime 内部已累加了历史值），直接存储
                        if i == msg_count - 1 && m.role == "assistant" && (agent_result.total_input_tokens > 0 || agent_result.total_output_tokens > 0) {
                            let _ = state.session_store.append_message(&session_id, &make_storage_msg_with_tokens(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone(), m.first_reasoning.clone(), m.again_reasonings.clone(), m.reasoning.clone(), agent_result.total_input_tokens, agent_result.total_output_tokens, agent_result.total_cached_tokens, agent_result.last_input_tokens, agent_result.last_output_tokens, agent_result.cache_hit_rate));
                        } else {
                            let _ = state.session_store.append_message(&session_id, &make_storage_msg_with_reasoning(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone(), m.first_reasoning.clone(), m.again_reasonings.clone(), m.reasoning.clone()));
                        }
                    }
                    drop(state);
                    if agent_result.cancelled {
                        let _ = agent_sse_tx.send(serde_json::json!({"type": "stopped", "data": {
                            "session_id": session_id,
                        }}).to_string()).await;
                    } else {
                        // input_tokens / output_tokens 已经是会话级累计值（runtime 内部已累加了历史值），直接使用
                        let _ = agent_sse_tx.send(serde_json::json!({"type": "done", "data": {
                            "content": agent_result.content,
                            "session_id": session_id,
                            "input_tokens": agent_result.total_input_tokens,
                            "output_tokens": agent_result.total_output_tokens,
                            "cached_tokens": agent_result.total_cached_tokens,
                            "cumulative_input_tokens": agent_result.total_input_tokens,
                            "cumulative_output_tokens": agent_result.total_output_tokens,
                            "last_input_tokens": agent_result.last_input_tokens,
                            "last_output_tokens": agent_result.last_output_tokens,
                            "cache_hit_rate": agent_result.cache_hit_rate,
                            "cache_hit_tokens": agent_result.total_cached_tokens,
                        }}).to_string()).await;
                    }
                }
                Err(e) => { let _ = agent_sse_tx.send(serde_json::json!({"type": "error", "data": {"message": e.to_string()}}).to_string()).await; }
            }
            // 从 cancel_map 中移除已完成的会话
            {
                let mut state = APP_STATE.write().await;
                state.cancel_map.remove(&session_id);
            }
        });
    });
    let stream: SseEventStream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(sse_rx).map(|data| Ok(Event::default().data(data))));
    return Sse::new(stream).keep_alive(KeepAlive::default());
}

async fn cancel_stream(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let session_id = match body.get("session_id").and_then(|v| v.as_str()) {
        Some(id) => id, None => return Json(serde_json::json!({"success": false, "message": "缺少 session_id 参数"})),
    };
    let state = APP_STATE.read().await;
    if let Some(flag) = state.cancel_map.get(session_id) {
        flag.store(true, Ordering::Relaxed);
        Json(serde_json::json!({"success": true, "message": "正在取消..."}))
    } else {
        let approve_keys: Vec<String> = state.cancel_map.keys().filter(|k| k.starts_with(&format!("approve:{}:", session_id))).cloned().collect();
        for key in &approve_keys { if let Some(flag) = state.cancel_map.get(key) { flag.store(true, Ordering::Relaxed); } }
        if approve_keys.is_empty() { Json(serde_json::json!({"success": true, "message": "无正在运行的任务"})) }
        else { Json(serde_json::json!({"success": true, "message": "正在取消..."})) }
    }
}

#[derive(serde::Deserialize)]
struct ApproveReq {
    approval_id: String,
    decision: String, // "allow_once" | "always_allow" | "deny"
}

/// 用户审批决策 API — 前端对话框点击后调用，释放阻塞的 runtime 循环
async fn approve_command(Json(req): Json<ApproveReq>) -> Json<serde_json::Value> {
    use crate::tools::approval::ApprovalDecision;
    let decision = match req.decision.as_str() {
        "allow_once" => ApprovalDecision::AllowOnce,
        "always_allow" => ApprovalDecision::AlwaysAllow,
        "deny" => ApprovalDecision::Deny,
        _ => return Json(serde_json::json!({"success": false, "message": "无效的决策参数"})),
    };
    let state = APP_STATE.read().await;
    let resolved = state.approval_manager.resolve_approval(&req.approval_id, decision).await;
    if resolved {
        tracing::info!("[Approve] 用户确认 approval_id={}, decision={}", req.approval_id, req.decision);
        Json(serde_json::json!({"success": true}))
    } else {
        Json(serde_json::json!({"success": false, "message": "未找到该审批请求或已超时"}))
    }
}

async fn test_connection(Json(req): Json<TestConnectionReq>) -> Json<serde_json::Value> {
    let provider = crate::config::ProviderConfig {
        name: "test".to_string(), api_key: req.api_key, base_url: req.base_url, models: vec![crate::config::ModelEntry::Name(req.model.clone())],
    };
    let client = match crate::llm::client::LlmClient::new(provider, 30) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"success": false, "message": e.to_string()})),
    };
    let chat_req = ChatRequest {
        model: req.model,
        messages: vec![crate::llm::types::ChatMessage { role: "user".to_string(), content: serde_json::Value::String("Hi".to_string()), tool_calls: None, tool_call_id: None, name: None, reasoning_content: None }],
        temperature: None, stream: false, tools: None, stream_options: None, extra_body: None,
    };
    match client.chat(&chat_req).await {
        Ok(_resp) => {
            Json(serde_json::json!({"success": true, "message": "连接成功"}))
        }
        Err(e) => Json(serde_json::json!({"success": false, "message": e.to_string()})),
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .route("/chat/cancel", post(cancel_stream))
        .route("/chat/approve", post(approve_command))
        .route("/chat/test", post(test_connection))
}
