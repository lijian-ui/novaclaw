use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::post,
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::Ordering;
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
    }
}

fn make_storage_msg_with_tokens(session_id: &str, role: &str, content: &str, tool_calls: Option<Vec<storage::ToolCall>>, tool_call_id: Option<String>, tool_name: Option<String>, input_tokens: u64, output_tokens: u64, cached_tokens: u64, last_input_tokens: u64, last_output_tokens: u64) -> storage::Message {
    storage::Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: None, tool_calls, tool_call_id, tool_name,
        first_reasoning: None, again_reasonings: None, reasoning: None,
        input_tokens: Some(input_tokens), output_tokens: Some(output_tokens), cached_tokens: Some(cached_tokens),
        last_input_tokens: Some(last_input_tokens), last_output_tokens: Some(last_output_tokens),
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
    let llm_client = crate::llm::client::LlmClient::new(provider, state.config.llm_timeout);
    let history = state.session_store.get_messages(&session_id).unwrap_or_default();
    let mut agent_session = AgentSession::new(&make_session_title(&req.message), &model, None);
    agent_session.id = session_id.clone();
    for m in &history { if m.role != "system" { agent_session.push_message(storage_msg_to_agent_msg(m)); } }

    let mut runtime = AgentRuntime::new(agent_session, llm_client, Arc::new(state.tool_registry.clone()), &state.config, state.skills_loader.list_skills());
    let result = match runtime.run_turn(&req.message, None, None, &[]).await {
        Ok(r) => r, Err(e) => return Json(serde_json::json!({"success": false, "message": e.to_string()})),
    };

    let history_len = state.session_store.get_messages(&session_id).unwrap_or_default().len();
    let _ = state.session_store.append_message(&session_id, &make_storage_msg(&session_id, "user", &req.message, None, None, None));
    let new_msgs = if history_len <= result.messages.len() { &result.messages[history_len..] } else { &result.messages[..] };
    for m in new_msgs {
        let tcs = m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| storage::ToolCall { id: tc.id.clone(), name: tc.name.clone(), arguments: Some(tc.arguments.clone()) }).collect());
        let _ = state.session_store.append_message(&session_id, &make_storage_msg(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone()));
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
        // 如果指定了智能体，从文件系统加载 SOUL.md 作为系统提示词
        if let Some(ref agent_id) = req.agent_id {
            let paths = crate::soul::SoulPaths::default();
            match crate::soul::AgentConfig::get_soul_content(&paths, agent_id) {
                Ok(soul_content) => {
                    agent_session.system_prompt_override = Some(soul_content);
                    tracing::info!("[Agent] 用户选择智能体: id={}", agent_id);
                }
                Err(_) => {
                    tracing::warn!("[Agent] 用户选择的智能体 '{}' 未找到 SOUL.md，使用默认提示词", agent_id);
                }
            }
        } else {
            tracing::debug!("[Agent] 使用默认智能体（未指定 agent_id）");
        }
        for m in &history { if m.role != "system" { agent_session.push_message(storage_msg_to_agent_msg(m)); } }

        let llm_client = crate::llm::client::LlmClient::new(provider, state.config.llm_timeout);
        let config = state.config.clone();
        let skills = state.skills_loader.list_skills();
        let tool_registry = Arc::new(state.tool_registry.clone());
        let history_msg_count = agent_session.messages.len();
        drop(state);

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

            let mut runtime = AgentRuntime::new(agent_session, llm_client, tool_registry, &config, skills);
            let result = runtime.run_turn(&req.message, Some(step_tx), None, &image_data_urls).await;
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
                        let tcs = m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| storage::ToolCall { id: tc.id.clone(), name: tc.name.clone(), arguments: Some(tc.arguments.clone()) }).collect());
                        // 仅最后一条 assistant 消息携带 Token 用量
                        if i == msg_count - 1 && m.role == "assistant" && (agent_result.total_input_tokens > 0 || agent_result.total_output_tokens > 0) {
                            let _ = state.session_store.append_message(&session_id, &make_storage_msg_with_tokens(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone(), agent_result.total_input_tokens, agent_result.total_output_tokens, agent_result.total_cached_tokens, agent_result.last_input_tokens, agent_result.last_output_tokens));
                        } else {
                            let _ = state.session_store.append_message(&session_id, &make_storage_msg(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone()));
                        }
                    }
                    drop(state);
                    let _ = agent_sse_tx.send(serde_json::json!({"type": "done", "data": {
                        "content": agent_result.content,
                        "session_id": session_id,
                        "input_tokens": agent_result.total_input_tokens,
                        "output_tokens": agent_result.total_output_tokens,
                        "cached_tokens": agent_result.total_cached_tokens,
                        "last_input_tokens": agent_result.last_input_tokens,
                        "last_output_tokens": agent_result.last_output_tokens,
                    }}).to_string()).await;
                }
                Err(e) => { let _ = agent_sse_tx.send(serde_json::json!({"type": "error", "data": {"message": e.to_string()}}).to_string()).await; }
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

async fn test_connection(Json(req): Json<TestConnectionReq>) -> Json<serde_json::Value> {
    let provider = crate::config::ProviderConfig {
        name: "test".to_string(), api_key: req.api_key, base_url: req.base_url, models: vec![req.model.clone()],
    };
    let client = crate::llm::client::LlmClient::new(provider, 30);
    let chat_req = ChatRequest {
        model: req.model,
        messages: vec![crate::llm::types::ChatMessage { role: "user".to_string(), content: serde_json::Value::String("Hi".to_string()), tool_calls: None, tool_call_id: None, name: None, reasoning_content: None }],
        temperature: None, stream: false, tools: None, stream_options: None,
    };
    match client.chat(&chat_req).await {
        Ok(resp) => {
            let msg = resp.choices.first().and_then(|c| c.message.as_ref()).and_then(|m| m.content.as_deref()).unwrap_or("");
            Json(serde_json::json!({"success": true, "message": msg}))
        },
        Err(e) => Json(serde_json::json!({"success": false, "message": e.to_string()})),
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .route("/chat/cancel", post(cancel_stream))
        .route("/chat/test", post(test_connection))
}
