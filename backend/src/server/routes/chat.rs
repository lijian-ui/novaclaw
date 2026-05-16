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

/// SSE 流类型别名（使用 Box 消除 async_stream 不同调用的类型差异）
type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

use crate::agent::runtime::AgentRuntime;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::storage;
use crate::tools::types::AgentStep;
use crate::APP_STATE;

#[derive(Deserialize)]
struct ChatRequest {
    #[serde(default)]
    session_id: Option<String>,
    message: String,
    #[serde(default)]
    model: Option<String>,
}

/// SSE 流式聊天请求
#[derive(Deserialize)]
struct ChatStreamRequest {
    #[serde(default)]
    session_id: Option<String>,
    message: String,
    #[serde(default)]
    model: Option<String>,
    /// 自定义工作目录路径（为空时使用默认 workspace）
    #[serde(default)]
    workspace: Option<String>,
}

#[derive(Deserialize)]
struct TestConnectionReq {
    api_key: String,
    base_url: String,
    model: String,
}

/// 从用户消息提取简洁标题
fn make_session_title(msg: &str) -> String {
    let cleaned: String = msg.chars().take(50).collect();
    let cleaned = cleaned.replace('\r', " ").replace('\n', " ");
    let trimmed = cleaned.trim().to_string();
    if trimmed.is_empty() {
        "新对话".to_string()
    } else {
        trimmed
    }
}

/// 将 storage::Message 转换为 AgentMessage
fn storage_msg_to_agent_msg(m: &storage::Message) -> AgentMessage {
    let tool_calls = m.tool_calls.as_ref().map(|tcs| {
        tcs.iter()
            .map(|tc| AgentToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone().unwrap_or_default(),
            })
            .collect()
    });
    AgentMessage {
        role: m.role.clone(),
        content: m.content.clone(),
        tool_calls,
        tool_call_id: m.tool_call_id.clone(),
        tool_name: m.tool_name.clone(),
        first_reasoning: m.first_reasoning.clone(),
        again_reasonings: m.again_reasonings.clone(),
        reasoning: m.reasoning.clone(),
    }
}

/// 非流式聊天（保持原有接口）
async fn chat(Json(req): Json<ChatRequest>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;

    let session_id = match &req.session_id {
        Some(id) => id.clone(),
        None => {
            let session_name = make_session_title(&req.message);
            match state.session_store.create_session(&session_name, req.model.as_deref()) {
                Ok(session) => session.id,
                Err(e) => {
                    return Json(serde_json::json!({ "success": false, "message": e.to_string() }))
                }
            }
        }
    };

    let now = chrono::Utc::now().to_rfc3339();

    let _ = state.session_store.append_message(
        &session_id,
        &storage::Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            role: "user".to_string(),
            content: req.message.clone(),
            created_at: now.clone(),
            metadata: None,
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            first_reasoning: None,
            again_reasonings: None,
            reasoning: None,
        },
    );

    let model = req.model.as_deref().unwrap_or(&state.models_config.default_model);
    let provider = match state.models_config.find_provider_by_model(model) {
        Some(p) => p.clone(),
        None => {
            return Json(serde_json::json!({
                "success": false,
                "message": format!("未找到模型 '{}' 的提供商配置", model)
            }));
        }
    };

    let llm_client = crate::llm::client::LlmClient::new(provider, state.config.llm_timeout);
    let agent_session = AgentSession::new(&format!("chat-{}", &session_id[..8]), model, None);
    let tool_registry = state.tool_registry.clone();
    let config = state.config.clone();
    let skills = state.skills_loader.list_skills();
    drop(state);

    let mut runtime = AgentRuntime::new(
        agent_session,
        llm_client,
        Arc::new(tool_registry),
        &config,
        skills,
    );

    match runtime.run_turn(&req.message, None, None).await {
        Ok(result) => {
            let state = APP_STATE.read().await;
            for agent_msg in &result.messages {
                if agent_msg.role == "user" {
                    continue;
                }
                let tool_calls = agent_msg.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| storage::ToolCall {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            arguments: Some(tc.arguments.clone()),
                        })
                        .collect()
                });
                let storage_msg = storage::Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session_id.clone(),
                    role: agent_msg.role.clone(),
                    content: agent_msg.content.clone(),
                    created_at: now.clone(),
                    metadata: None,
                    tool_calls,
                    tool_call_id: agent_msg.tool_call_id.clone(),
                    tool_name: agent_msg.tool_name.clone(),
                    first_reasoning: agent_msg.first_reasoning.clone(),
                    again_reasonings: agent_msg.again_reasonings.clone(),
                    reasoning: agent_msg.reasoning.clone(),
                };
                let _ = state.session_store.append_message(&session_id, &storage_msg);
            }
            drop(state);

            Json(serde_json::json!({
                "success": true,
                "data": {
                    "session_id": session_id,
                    "message_id": uuid::Uuid::new_v4().to_string(),
                    "content": result.content,
                    "role": "assistant",
                }
            }))
        }
        Err(e) => Json(serde_json::json!({ "success": false, "message": e.to_string() })),
    }
}

/// SSE 流式聊天端点
/// POST /api/chat/stream
///
/// 请求体: { "message": "...", "model": "...", "session_id": "..." }
/// 响应: text/event-stream
///
/// 事件格式（保持与之前 WebSocket 协议兼容的 JSON 结构）:
///
///   event: message
///   data: {"type":"agent_step","data":{"step_type":"reasoning","content":"..."}}
///
///   event: message
///   data: {"type":"agent_step","data":{"step_type":"first_thought","content":"..."}}
///
///   event: message
///   data: {"type":"agent_step","data":{"step_type":"tool_call","content":"...","tool_name":"read_file"}}
///
///   event: message
///   data: {"type":"chunk","data":"文本片段"}
///
///   event: message
///   data: {"type":"done","data":{"session_id":"...","content":"...","iterations":5}}
///
///   event: message
///   data: {"type":"error","data":{"message":"错误信息"}}
async fn chat_stream(
    Json(req): Json<ChatStreamRequest>,
) -> Sse<SseEventStream> {
    let user_message = req.message;
    let model_name = req.model;
    let session_id_opt = req.session_id;
    let custom_workspace = req.workspace;

    // 创建 SSE 事件通道
    let (sse_tx, mut sse_rx) = mpsc::channel::<String>(256);
    // 创建 Agent 步骤通道
    let (step_tx, mut step_rx) = mpsc::channel::<AgentStep>(32);

    // 获取配置并构建 Agent
    let (config, models_config, tool_registry, skills, session_store) = {
        let state = APP_STATE.read().await;
        (
            state.config.clone(),
            state.models_config.clone(),
            state.tool_registry.clone(),
            state.skills_loader.list_skills(),
            state.session_store.clone(),
        )
    };

    let model = model_name.unwrap_or_else(|| models_config.default_model.clone());
    let provider = match models_config.find_provider_by_model(&model) {
        Some(p) => p.clone(),
        None => {
            let err_json = serde_json::json!({
                "type": "error",
                "data": {"message": format!("未找到模型 '{}' 的提供商配置", model)}
            })
            .to_string();
            let stream: SseEventStream = Box::pin(futures::stream::once(async move {
                Ok::<_, Infallible>(Event::default().data(err_json))
            }));
            return Sse::new(stream).keep_alive(KeepAlive::default());
        }
    };

    let llm_client = crate::llm::client::LlmClient::new(provider, config.llm_timeout);

    // 加载历史消息恢复上下文
    let (agent_session, resolved_sid) = if let Some(ref sid) = session_id_opt {
        match session_store.get_session(sid) {
            Ok(existing_session) => {
                let history = session_store.get_messages(sid).unwrap_or_default();
                tracing::info!("[SSE Chat] 恢复会话 {} 的历史上下文，共 {} 条消息", sid, history.len());
                let mut session = AgentSession::new(&existing_session.name, &model, existing_session.metadata.as_deref());
                session.id = existing_session.id.clone();
                for m in &history {
                    if m.role != "system" {
                        session.push_message(storage_msg_to_agent_msg(m));
                    }
                }
                (session, sid.clone())
            }
            Err(_) => {
                tracing::warn!("[SSE Chat] session_id {} 不存在，创建新会话", sid);
                let session = AgentSession::new(&make_session_title(&user_message), &model, custom_workspace.as_deref());
                let new_sid = session.id.clone();
                (session, new_sid)
            }
        }
    } else {
        tracing::info!("[SSE Chat] 首次对话，创建新 AgentSession");
        let session = AgentSession::new(&make_session_title(&user_message), &model, custom_workspace.as_deref());
        let new_sid = session.id.clone();
        (session, new_sid)
    };

    let history_msg_count = agent_session.messages.len();
    tracing::info!("[SSE Chat] 初始化完成，历史消息数: {}", history_msg_count);

    // 注册取消标志
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut state = APP_STATE.write().await;
        state.cancel_map.insert(resolved_sid.clone(), cancel_flag.clone());
    }

    // 前向任务: AgentStep → SSE 事件
    let step_sse_tx = sse_tx.clone();
    let step_fwd_handle = tokio::spawn(async move {
        while let Some(step) = step_rx.recv().await {
            let event_json = match step.step_type.as_str() {
                "text_chunk" => serde_json::json!({
                    "type": "chunk",
                    "data": step.content,
                }),
                _ => serde_json::json!({
                    "type": "agent_step",
                    "data": {
                        "step_type": step.step_type,
                        "content": step.content,
                        "tool_name": step.tool_name,
                        "tool_result": step.tool_result,
                        "turn": step.turn,
                        "max_turns": step.max_turns,
                    }
                }),
            };
            if step_sse_tx.send(event_json.to_string()).await.is_err() {
                break;
            }
        }
    });

    // Agent 任务: 执行 run_turn，等待 step 全部转发完成后再发送 done
    let agent_sse_tx = sse_tx.clone();
    tokio::spawn(async move {
        let mut runtime = AgentRuntime::new(agent_session, llm_client, Arc::new(tool_registry), &config, skills);

        let result = runtime.run_turn(&user_message, Some(step_tx), Some(cancel_flag)).await;

        // 关键修复：等待 step_forward 转发完所有 AgentStep 事件
        // 确保 done 事件在 first_thought/thought/tool_call 之后到达前端
        let _ = step_fwd_handle.await;

        // 清理取消标志
        {
            let mut state = APP_STATE.write().await;
            state.cancel_map.remove(&resolved_sid);
        }

        match result {
            Ok(agent_result) => {
                // 处理上下文压缩导致的 slice 越界问题
                // 当 history_msg_count > messages.len() 时，说明 run_turn 中触发了
                // 上下文压缩（COMPACT_THRESHOLD=40），历史消息被压缩到 COMPACT_KEEP_LAST 条
                let new_messages = if history_msg_count <= agent_result.messages.len() {
                    &agent_result.messages[history_msg_count..]
                } else {
                    // 压缩后保留了末尾 COMPACT_KEEP_LAST 条历史消息，新消息紧随其后
                    &agent_result.messages[crate::agent::runtime::COMPACT_KEEP_LAST_FALLBACK..]
                };

                // 持久化消息
                let now = chrono::Utc::now().to_rfc3339();
                let state = APP_STATE.read().await;
                let final_sid = if session_store.get_session(&resolved_sid).is_ok() {
                    resolved_sid.clone()
                } else {
                    // 会话不存在，创建新会话
                    match session_store.create_session(&make_session_title(&user_message), Some(&model)) {
                        Ok(s) => s.id,
                        Err(e) => {
                            let err_json = serde_json::json!({
                                "type": "error",
                                "data": {"message": format!("创建会话失败: {}", e)}
                            }).to_string();
                            let _ = agent_sse_tx.send(err_json).await;
                            return;
                        }
                    }
                };

                for agent_msg in new_messages {
                    let tool_calls = agent_msg.tool_calls.as_ref().map(|tcs| {
                        tcs.iter()
                            .map(|tc| storage::ToolCall {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                arguments: Some(tc.arguments.clone()),
                            })
                            .collect()
                    });
                    let storage_msg = storage::Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        session_id: final_sid.clone(),
                        role: agent_msg.role.clone(),
                        content: agent_msg.content.clone(),
                        created_at: now.clone(),
                        metadata: None,
                        tool_calls,
                        tool_call_id: agent_msg.tool_call_id.clone(),
                        tool_name: agent_msg.tool_name.clone(),
                        first_reasoning: agent_msg.first_reasoning.clone(),
                        again_reasonings: agent_msg.again_reasonings.clone(),
                        reasoning: agent_msg.reasoning.clone(),
                    };
                    let _ = session_store.append_message(&final_sid, &storage_msg);
                }
                drop(state);

                if agent_result.cancelled {
                    let _ = agent_sse_tx
                        .send(
                            serde_json::json!({
                                "type": "stopped",
                                "data": {"session_id": final_sid, "reason": "user_cancel"}
                            })
                            .to_string(),
                        )
                        .await;
                } else {
                    let _ = agent_sse_tx
                        .send(
                            serde_json::json!({
                                "type": "done",
                                "data": {
                                    "session_id": final_sid,
                                    "content": agent_result.content,
                                    "iterations": agent_result.iterations,
                                    "max_iterations_reached": agent_result.max_iterations_reached,
                                }
                            })
                            .to_string(),
                        )
                        .await;
                }
            }
            Err(e) => {
                let _ = agent_sse_tx
                    .send(
                        serde_json::json!({
                            "type": "error",
                            "data": {"message": e.to_string()}
                        })
                        .to_string(),
                    )
                    .await;
            }
        }
    });

    // drop 原始的 sse_tx，这样当所有发送者都被 drop 后，sse_rx 会自动关闭
    drop(sse_tx);

    // 构建 SSE 响应流（转换为 boxed stream 以统一类型）
    let stream: SseEventStream = Box::pin(async_stream::stream! {
        while let Some(data) = sse_rx.recv().await {
            yield Ok::<_, Infallible>(Event::default().data(data));
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// 取消正在进行的流式生成
/// POST /api/chat/cancel  body: { "session_id": "..." }
#[derive(Deserialize)]
struct CancelReq { session_id: String }

async fn cancel_stream(Json(req): Json<CancelReq>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;
    if let Some(cancel_flag) = state.cancel_map.get(&req.session_id) {
        cancel_flag.store(true, Ordering::Relaxed);
        drop(state);
        tracing::info!("[SSE Chat] 取消会话 {} 的流式生成", req.session_id);
        Json(serde_json::json!({
            "success": true,
            "message": "已发送取消信号"
        }))
    } else {
        drop(state);
        Json(serde_json::json!({
            "success": false,
            "message": "未找到该会话的流式生成"
        }))
    }
}

/// 测试提供商连接
async fn test_connection(Json(req): Json<TestConnectionReq>) -> Json<serde_json::Value> {
    let normalized_url = crate::llm::client::normalize_base_url(&req.base_url);

    let client = crate::llm::client::LlmClient::new(
        crate::config::ProviderConfig {
            name: "test".to_string(),
            api_key: req.api_key.clone(),
            base_url: normalized_url.clone(),
            models: vec![req.model.clone()],
        },
        10,
    );

    let chat_req = crate::llm::types::ChatRequest {
        model: req.model.clone(),
        messages: vec![crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: "test connection".to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }],
        temperature: Some(0.0),
        stream: false,
        tools: None,
        stream_options: None,
    };

    match client.chat(&chat_req).await {
        Ok(resp) => {
            let model_returned = &resp.model;
            let content = resp.choices.first()
                .and_then(|c| c.message.as_ref())
                .and_then(|m| m.content.as_deref())
                .unwrap_or("(空响应)");
            Json(serde_json::json!({
                "success": true,
                "message": format!("连接成功！模型: {}, 响应: {}", model_returned, &content[..content.len().min(80)]),
            }))
        }
        Err(e) => {
            let err_msg = e.to_string();
            let hint = if normalized_url != req.base_url {
                format!("\n\n注意: 原始 base_url '{}' 已自动修正为 '{}'，请确认此地址正确",
                    req.base_url, normalized_url)
            } else {
                String::new()
            };
            Json(serde_json::json!({
                "success": false,
                "message": format!("{}{}", err_msg, hint),
            }))
        }
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .route("/chat/cancel", post(cancel_stream))
        .route("/chat/test", post(test_connection))
}