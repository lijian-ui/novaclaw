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
    let result = match runtime.run_turn(&req.message, None, None).await {
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
            let mut runtime = AgentRuntime::new(agent_session, llm_client, tool_registry, &config, skills);
            let result = runtime.run_turn(&req.message, Some(step_tx), None).await;
            let _ = step_fwd_handle.await;

            match result {
                Ok(agent_result) => {
                    let state = APP_STATE.read().await;
                    let _ = state.session_store.append_message(&session_id, &make_storage_msg(&session_id, "user", &req.message, None, None, None));
                    let new_msgs = if history_msg_count <= agent_result.messages.len() { &agent_result.messages[history_msg_count..] }
                        else { let keep = crate::agent::runtime::COMPACT_KEEP_LAST_FALLBACK; if agent_result.messages.len() > keep { &agent_result.messages[agent_result.messages.len() - keep..] } else { &agent_result.messages[..] } };
                    for m in new_msgs {
                        let tcs = m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| storage::ToolCall { id: tc.id.clone(), name: tc.name.clone(), arguments: Some(tc.arguments.clone()) }).collect());
                        let _ = state.session_store.append_message(&session_id, &make_storage_msg(&session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone()));
                    }
                    drop(state);
                    let _ = agent_sse_tx.send(serde_json::json!({"type": "done", "data": {"content": agent_result.content, "session_id": session_id}}).to_string()).await;
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
        messages: vec![crate::llm::types::ChatMessage { role: "user".to_string(), content: "Hi".to_string(), tool_calls: None, tool_call_id: None, name: None, reasoning_content: None }],
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

#[derive(Deserialize)]
struct ApproveReq { approval_id: String, approved: bool, session_id: String }

async fn approve_tool(Json(req): Json<ApproveReq>) -> Sse<SseEventStream> {
    let (sse_tx, sse_rx) = mpsc::channel::<String>(256);
    let (step_tx, mut step_rx) = mpsc::channel::<AgentStep>(32);
    let sse_tx_clone = sse_tx.clone();

    tokio::spawn(async move {
        // Atomic take → 防止并发重复确认
        let pending = { let state = APP_STATE.read().await; state.approval_manager.take_pending(&req.approval_id).await };
        let (_approval, session_id_from_pending, tool_name, args_json) = match pending {
            Some(p) => p,
            None => { let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": "未找到该确认请求，可能已过期"}}).to_string()).await; return; }
        };
        if session_id_from_pending != req.session_id {
            let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": "会话 ID 不匹配"}}).to_string()).await; return;
        }

        let (config, models_config, tool_registry, skills, session_store) = {
            let state = APP_STATE.read().await;
            (state.config.clone(), state.models_config.clone(), Arc::new(state.tool_registry.clone()), state.skills_loader.list_skills(), state.session_store.clone())
        };

        if !req.approved {
            { let mut state = APP_STATE.write().await; state.cancel_map.remove(&format!("approve:{}:{}", req.session_id, req.approval_id)); }
            let _ = sse_tx_clone.send(serde_json::json!({"type": "approval_result", "data": {"approved": false, "message": "操作已取消"}}).to_string()).await;
            return;
        }

        // 执行工具
        let session_workspace = session_store.get_session(&req.session_id).ok().and_then(|s| s.metadata);
        let tool_output = match tool_name.as_str() {
            "delete_file" => {
                let args: serde_json::Value = match serde_json::from_str(&args_json) {
                    Ok(a) => a,
                    Err(e) => { let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": format!("参数解析失败: {}", e)}}).to_string()).await; return; }
                };
                crate::tools::approval::execute_delete_file(args, session_workspace.as_deref()).await
            }
            _ => { let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": format!("未知工具: {}", tool_name)}}).to_string()).await; return; }
        };
        let tool_output = match tool_output { Ok(o) => o, Err(e) => { let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": format!("执行失败: {}", e)}}).to_string()).await; return; } };

        let _ = sse_tx_clone.send(serde_json::json!({"type": "approval_result", "data": {"approved": true, "tool_name": tool_name, "output": tool_output.clone(), "message": "操作已执行"}}).to_string()).await;

        // 继续 Agent 执行
        let existing_session = match session_store.get_session(&req.session_id) {
            Ok(s) => s,
            Err(_) => { let _ = sse_tx_clone.send(serde_json::json!({"type": "done", "data": {"session_id": req.session_id}}).to_string()).await; return; }
        };
        let model = existing_session.model.clone();
        let provider = match models_config.find_provider_by_model(&model) {
            Some(p) => p.clone(),
            None => { let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": format!("未找到模型 '{}' 的提供商配置", model)}}).to_string()).await; return; }
        };
        let llm_client = crate::llm::client::LlmClient::new(provider, config.llm_timeout);
        let mut agent_session = AgentSession::new(&existing_session.name, &model, existing_session.metadata.as_deref());
        agent_session.id = existing_session.id.clone();
        let history = session_store.get_messages(&req.session_id).unwrap_or_default();
        for m in &history { if m.role != "system" { agent_session.push_message(storage_msg_to_agent_msg(m)); } }

        let continue_prompt = format!("工具 {} 执行完成，结果：\n{}", tool_name, tool_output);
        let step_sse_tx = sse_tx_clone.clone();
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

        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_key = format!("approve:{}:{}", req.session_id, req.approval_id);
        { let mut state = APP_STATE.write().await; state.cancel_map.insert(cancel_key.clone(), cancel_flag.clone()); }

        let history_msg_count = agent_session.messages.len();
        let mut runtime = AgentRuntime::new(agent_session, llm_client, tool_registry, &config, skills);
        let agent_result = runtime.run_turn(&continue_prompt, Some(step_tx), Some(cancel_flag)).await;
        let _ = step_fwd_handle.await;
        { let mut state = APP_STATE.write().await; state.cancel_map.remove(&cancel_key); }

        match agent_result {
            Ok(ar) => {
                let new_msgs = if history_msg_count <= ar.messages.len() { &ar.messages[history_msg_count..] }
                    else { &ar.messages[crate::agent::runtime::COMPACT_KEEP_LAST_FALLBACK..] };
                for m in new_msgs {
                    let tcs = m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| storage::ToolCall { id: tc.id.clone(), name: tc.name.clone(), arguments: Some(tc.arguments.clone()) }).collect());
                    let _ = session_store.append_message(&req.session_id, &make_storage_msg(&req.session_id, &m.role, &m.content, tcs, m.tool_call_id.clone(), m.tool_name.clone()));
                }
                let _ = sse_tx_clone.send(serde_json::json!({"type": "done", "data": {"content": ar.content, "session_id": req.session_id}}).to_string()).await;
            }
            Err(e) => { let _ = sse_tx_clone.send(serde_json::json!({"type": "error", "data": {"message": e.to_string()}}).to_string()).await; }
        }
    });
    let stream: SseEventStream = Box::pin(tokio_stream::wrappers::ReceiverStream::new(sse_rx).map(|data| Ok(Event::default().data(data))));
    return Sse::new(stream).keep_alive(KeepAlive::default());
}

pub fn routes() -> Router {
    Router::new()
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .route("/chat/cancel", post(cancel_stream))
        .route("/chat/test", post(test_connection))
        .route("/chat/approve", post(approve_tool))
}
