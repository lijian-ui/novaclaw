use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::agent::runtime::AgentRuntime;
use crate::agent::session::{AgentMessage, AgentSession, AgentToolCall};
use crate::storage;
use crate::APP_STATE;

/// WebSocket 升级处理
async fn ws_chat_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_chat_socket(socket))
}

/// 从用户消息提取简洁标题
fn make_session_title(msg: &str) -> String {
    let cleaned: String = msg.chars().take(50).collect();
    let cleaned = cleaned.replace('\r', " ").replace('\n', " ");
    let trimmed = cleaned.trim().to_string();
    if trimmed.is_empty() { "新对话".to_string() } else { trimmed }
}

/// 将 storage::Message 转换为 AgentMessage（用于恢复历史上下文）
fn storage_msg_to_agent_msg(m: &storage::Message) -> AgentMessage {
    let tool_calls = m.tool_calls.as_ref().map(|tcs| {
        tcs.iter().map(|tc| AgentToolCall {
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone().unwrap_or_default(),
        }).collect()
    });
    AgentMessage {
        role: m.role.clone(),
        content: m.content.clone(),
        tool_calls,
        tool_call_id: m.tool_call_id.clone(),
        tool_name: m.tool_name.clone(),
        first_reasoning: m.first_reasoning.clone(),
        reasonings: m.reasonings.clone(),
        reasoning: m.reasoning.clone(),
    }
}

/// 处理单个 WebSocket 连接的 ReAct 对话
async fn handle_chat_socket(socket: WebSocket) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 等待第一条消息（session info）
    let init_msg = match ws_receiver.next().await {
        Some(Ok(Message::Text(text))) => text,
        _ => {
            tracing::warn!("[Chat] 客户端未发送初始化消息，关闭连接");
            return;
        }
    };

    // 解析初始化消息: {"type":"send","data":{"message":"...","model":"...","session_id":"..."}}
    let (user_message, model_name, session_id_opt) =
        match serde_json::from_str::<serde_json::Value>(&init_msg) {
            Ok(val) => {
                let msg = val["data"]["message"].as_str().unwrap_or("").to_string();
                let model = val["data"]["model"].as_str().map(|s| s.to_string());
                let sid = val["data"]["session_id"].as_str().map(|s| s.to_string());
                (msg, model, sid)
            }
            Err(_) => {
                let _ = ws_sender
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "data": {"message": "无效的消息格式"}
                        })
                        .to_string(),
                    ))
                    .await;
                return;
            }
        };

    if user_message.is_empty() {
        let _ = ws_sender
            .send(Message::Text(
                serde_json::json!({
                    "type": "error",
                    "data": {"message": "消息不能为空"}
                })
                .to_string(),
            ))
            .await;
        return;
    }

    tracing::info!(
        "[Chat] 收到消息: {:?}，session_id: {:?}",
        &user_message.chars().take(60).collect::<String>(),
        session_id_opt
    );

    // 获取配置并构建 Agent
    let state = APP_STATE.read().await;
    let config = state.config.clone();
    let models_config = state.models_config.clone();
    let tool_registry = Arc::new(state.tool_registry.clone());
    let skills = state.skills_loader.list_skills();

    let model = model_name.unwrap_or_else(|| models_config.default_model.clone());
    let provider = match models_config.find_provider_by_model(&model) {
        Some(p) => p.clone(),
        None => {
            let _ = ws_sender
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "data": {"message": format!("未找到模型 '{}' 的提供商配置", model)}
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
    };

    let llm_client = crate::llm::client::LlmClient::new(provider, config.llm_timeout);

    // ── 核心修复：加载历史消息恢复上下文 ──
    // 如果前端传来了 session_id，从 SessionStore 加载历史消息注入到 AgentSession
    let (agent_session, resolved_sid_for_save) = if let Some(ref sid) = session_id_opt {
        match state.session_store.get_session(sid) {
            Ok(existing_session) => {
                // 加载该会话的历史消息
                let history = state.session_store.get_messages(sid).unwrap_or_default();
                tracing::info!(
                    "[Chat] 恢复会话 {} 的历史上下文，共 {} 条消息",
                    sid,
                    history.len()
                );

                // 构建带历史消息的 AgentSession
                let mut session = AgentSession::new(
                    &existing_session.name,
                    &model,
                    existing_session.metadata.as_deref(),
                );
                session.id = existing_session.id.clone();

                // 将历史消息注入 session（跳过 system 角色，system prompt 由 runtime 动态构建）
                for m in &history {
                    if m.role != "system" {
                        session.push_message(storage_msg_to_agent_msg(m));
                    }
                }

                (session, sid.clone())
            }
            Err(_) => {
                // session_id 不存在，创建新会话
                tracing::warn!("[Chat] session_id {} 不存在，创建新会话", sid);
                let session = AgentSession::new(
                    &make_session_title(&user_message),
                    &model,
                    None,
                );
                let new_sid = session.id.clone();
                (session, new_sid)
            }
        }
    } else {
        // 首次对话，创建新 AgentSession
        tracing::info!("[Chat] 首次对话，创建新 AgentSession");
        let session = AgentSession::new(
            &make_session_title(&user_message),
            &model,
            None,
        );
        let new_sid = session.id.clone();
        (session, new_sid)
    };

    drop(state);

    // 记录本轮对话开始前的消息数量，用于后续只保存新增消息
    let history_msg_count = agent_session.messages.len();
    tracing::info!(
        "[Chat] AgentSession 初始化完成，历史消息数: {}，session_id: {}",
        history_msg_count,
        resolved_sid_for_save
    );

    // 创建 Agent 步骤通道
    let (step_tx, mut step_rx) = mpsc::channel::<crate::tools::types::AgentStep>(32);

    let mut runtime = AgentRuntime::new(
        agent_session,
        llm_client,
        tool_registry,
        &config,
        skills,
    );

    // ---- 打断/停止信号 ----
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));

    // Agent 任务
    let agent_task = {
        let sender = Arc::clone(&ws_sender);
        let user_msg = user_message.clone();
        let sid_for_save = resolved_sid_for_save.clone();
        let sid_opt = session_id_opt.clone();
        let cancel = Arc::clone(&cancel_flag);

        tokio::spawn(async move {
            let result = runtime.run_turn(&user_msg, Some(step_tx), Some(cancel)).await;
            let model_name = runtime.session().model.clone();

            let mut sender = sender.lock().await;
            match result {
                Ok(agent_result) => {
                    let state = APP_STATE.read().await;

                    // ── 确保 SessionStore 中存在该会话 ──
                    let final_sid = if sid_opt.is_some() {
                        // 已有会话：检查是否存在，不存在则创建
                        if state.session_store.get_session(&sid_for_save).is_err() {
                            match state.session_store.create_session(
                                &make_session_title(&user_msg),
                                Some(&model_name),
                            ) {
                                Ok(s) => {
                                    tracing::info!("[Chat] 重新创建会话: {}", s.id);
                                    s.id
                                }
                                Err(e) => {
                                    tracing::error!("[Chat] 创建会话失败: {}", e);
                                    let _ = sender.send(Message::Text(
                                        serde_json::json!({"type":"error","data":{"message":format!("创建会话失败: {}", e)}}).to_string()
                                    )).await;
                                    return;
                                }
                            }
                        } else {
                            // 更新会话标题（如果还是默认标题）
                            if let Ok(mut s) = state.session_store.get_session(&sid_for_save) {
                                if s.name == "新任务" || s.name == "New Session" {
                                    s.name = make_session_title(&user_msg);
                                    let _ = state.session_store.update_session(&s);
                                }
                            }
                            sid_for_save.clone()
                        }
                    } else {
                        // 首次对话：创建新会话
                        match state.session_store.create_session(
                            &make_session_title(&user_msg),
                            Some(&model_name),
                        ) {
                            Ok(s) => {
                                tracing::info!("[Chat] 首次对话，创建会话: {}", s.id);
                                s.id
                            }
                            Err(e) => {
                                tracing::error!("[Chat] 创建会话失败: {}", e);
                                let _ = sender.send(Message::Text(
                                    serde_json::json!({"type":"error","data":{"message":format!("创建会话失败: {}", e)}}).to_string()
                                )).await;
                                return;
                            }
                        }
                    };

                    // ── 核心修复：只保存本轮新增的消息，不重复保存历史 ──
                    let new_messages = &agent_result.messages[history_msg_count..];
                    tracing::info!(
                        "[Chat] 本轮新增 {} 条消息，准备持久化到会话 {}",
                        new_messages.len(),
                        final_sid
                    );

                    let now = chrono::Utc::now().to_rfc3339();
                    for agent_msg in new_messages {
                        let tool_calls = agent_msg.tool_calls.as_ref().map(|tcs| {
                            tcs.iter().map(|tc| storage::ToolCall {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                arguments: Some(tc.arguments.clone()),
                            }).collect()
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
                            reasonings: agent_msg.reasonings.clone(),
                            reasoning: agent_msg.reasoning.clone(),
                        };

                        if let Err(e) = state.session_store.append_message(&final_sid, &storage_msg) {
                            tracing::error!("[Chat] 保存消息失败: {} (会话: {})", e, final_sid);
                        }
                    }
                    drop(state);

                    tracing::info!(
                        "[Chat] 持久化完成: 会话 {}，本轮 {} 条新消息，共 {} 次迭代",
                        final_sid,
                        new_messages.len(),
                        agent_result.iterations
                    );

                    if agent_result.cancelled {
                        let _ = sender.send(Message::Text(
                            serde_json::json!({
                                "type": "stopped",
                                "data": {
                                    "session_id": final_sid,
                                    "reason": "user_cancel",
                                }
                            }).to_string(),
                        )).await;
                    } else {
                        let _ = sender.send(Message::Text(
                            serde_json::json!({
                                "type": "done",
                                "data": {
                                    "session_id": final_sid,
                                    "content": agent_result.content,
                                    "iterations": agent_result.iterations,
                                    "max_iterations_reached": agent_result.max_iterations_reached,
                                }
                            }).to_string(),
                        )).await;
                    }
                }
                Err(e) => {
                    tracing::error!("[Chat] Agent 执行失败: {}", e);
                    let _ = sender.send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "data": {"message": e.to_string()}
                        }).to_string(),
                    )).await;
                }
            }
        })
    };

    // 步骤转发任务：将 Agent 步骤转发到 WebSocket
    let step_forward = {
        let sender = Arc::clone(&ws_sender);
        tokio::spawn(async move {
            while let Some(step) = step_rx.recv().await {
                let mut sender = sender.lock().await;
                match step.step_type.as_str() {
                    "text_chunk" => {
                        let _ = sender.send(Message::Text(
                            serde_json::json!({
                                "type": "chunk",
                                "data": step.content,
                            }).to_string(),
                        )).await;
                    }
                    _ => {
                        let _ = sender.send(Message::Text(
                            serde_json::json!({
                                "type": "agent_step",
                                "data": {
                                    "step_type": step.step_type,
                                    "content": step.content,
                                    "tool_name": step.tool_name,
                                    "tool_result": step.tool_result.map(|r|
                                        r[..r.len().min(500)].to_string()
                                    ),
                                    "turn": step.turn,
                                    "max_turns": step.max_turns,
                                }
                            }).to_string(),
                        )).await;
                    }
                }
                drop(sender);
            }
        })
    };

    // 监听客户端消息 — 支持打断/停止
    let cancel_flag_clone = Arc::clone(&cancel_flag);
    let sender_for_cancel = Arc::clone(&ws_sender);
    let cancel_listener = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if text.contains("\"type\":\"stop\"") {
                        tracing::info!("[Chat] 用户请求停止生成");
                        cancel_flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                        let mut sender = sender_for_cancel.lock().await;
                        let _ = sender.send(Message::Text(
                            serde_json::json!({
                                "type": "stopped",
                                "data": {"reason": "user_cancel"}
                            }).to_string(),
                        )).await;
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    let _ = agent_task.await;
    step_forward.abort();
    cancel_listener.abort();
}

pub fn routes() -> Router {
    Router::new().route("/chat", get(ws_chat_handler))
}
