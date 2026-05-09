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
use crate::agent::AgentSession;
use crate::storage;
use crate::APP_STATE;

/// WebSocket 升级处理
async fn ws_chat_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_chat_socket(socket))
}

/// 处理单个 WebSocket 连接的 ReAct 对话
async fn handle_chat_socket(socket: WebSocket) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 等待第一条消息（session info）
    let init_msg = match ws_receiver.next().await {
        Some(Ok(Message::Text(text))) => text,
        _ => {
            tracing::warn!("客户端未发送初始化消息，关闭连接");
            return;
        }
    };

    // 解析初始化消息: {"type":"send","data":{"message":"...","model":"...","session_id":"..."}}
    let (user_message, model_name, session_id_opt) = match serde_json::from_str::<serde_json::Value>(&init_msg)
    {
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

    tracing::info!("WebSocket 对话: {}", &user_message.chars().take(50).collect::<String>());

    // 从用户消息提取简洁标题
    fn make_session_title(msg: &str) -> String {
        let cleaned: String = msg.chars().take(50).collect();
        let cleaned = cleaned.replace('\r', " ").replace('\n', " ");
        let trimmed = cleaned.trim().to_string();
        if trimmed.is_empty() { "新对话".to_string() } else { trimmed }
    }

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

    let agent_session = AgentSession::new(
        &format!("ws-{}", &uuid::Uuid::new_v4().to_string()[..8]),
        &model,
        None,
    );

    drop(state);

    // 创建 Agent 步骤通道
    let (step_tx, mut step_rx) = mpsc::channel::<crate::tools::types::AgentStep>(32);

    let mut runtime = AgentRuntime::new(
        agent_session,
        llm_client,
        tool_registry,
        &config,
        skills,
    );

    let _max_turns = config.max_iterations;

    // ---- 打断/停止信号 ----
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // 并行处理：一个任务运行 Agent，另一个任务将结果推送到 WebSocket
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));

    // Agent 任务（传递 cancel_flag）
    let agent_task = {
        let sender = Arc::clone(&ws_sender);
        let user_msg = user_message.clone();
        let sid = session_id_opt.clone();
        let cancel = Arc::clone(&cancel_flag);
        tokio::spawn(async move {
            let mut _build_tx = None;
            _build_tx = Some(step_tx);

            let result = runtime.run_turn(&user_msg, _build_tx, Some(cancel)).await;
            let model = runtime.session().model.clone();

            // 发送最终结果
            let mut sender = sender.lock().await;
            match result {
                Ok(agent_result) => {
                    // ---- 持久化会话消息到 SessionStore ----
                    let state = APP_STATE.read().await;

                    // 解析或创建会话
                    let session_title = make_session_title(&user_msg);
                    let resolved_sid = if let Some(ref existing_sid) = sid {
                        if let Ok(mut existing_session) = state.session_store.get_session(existing_sid) {
                            if existing_session.name == "新任务" || existing_session.name == "New Session" {
                                existing_session.name = session_title.clone();
                                let _ = state.session_store.update_session(&existing_session);
                            }
                            existing_sid.clone()
                        } else {
                            match state.session_store.create_session(&session_title, Some(&model)) {
                                Ok(s) => s.id,
                                Err(e) => {
                                    let _ = sender
                                        .send(Message::Text(
                                            serde_json::json!({
                                                "type": "error",
                                                "data": {"message": format!("创建会话失败: {}", e)}
                                            })
                                            .to_string(),
                                        ))
                                        .await;
                                    return;
                                }
                            }
                        }
                    } else {
                        match state.session_store.create_session(&session_title, Some(&model)) {
                            Ok(s) => s.id,
                            Err(e) => {
                                let _ = sender
                                    .send(Message::Text(
                                        serde_json::json!({
                                            "type": "error",
                                            "data": {"message": format!("创建会话失败: {}", e)}
                                        })
                                        .to_string(),
                                    ))
                                    .await;
                                return;
                            }
                        }
                    };

                    // 保存所有对话消息到 SessionStore
                    let now = chrono::Utc::now().to_rfc3339();
                    for agent_msg in &agent_result.messages {
                        // 转换 tool_calls 格式
                        let tool_calls = agent_msg.tool_calls.as_ref().map(|tcs| {
                            tcs.iter().map(|tc| storage::ToolCall {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                arguments: Some(tc.arguments.clone()),
                            }).collect()
                        });

                        let storage_msg = storage::Message {
                            id: uuid::Uuid::new_v4().to_string(),
                            session_id: resolved_sid.clone(),
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
                        if let Err(e) = state.session_store.append_message(&resolved_sid, &storage_msg) {
                            tracing::error!("保存消息到会话失败: {} (会话: {})", e, resolved_sid);
                        }
                    }
                    drop(state);

                    // 判断是否被用户打断
                    if agent_result.cancelled {
                        // 已保存部分输出，发送 stopped
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "stopped",
                                    "data": {
                                        "session_id": resolved_sid,
                                        "reason": "user_cancel",
                                    }
                                })
                                .to_string(),
                            ))
                            .await;
                    } else {
                        tracing::info!("会话消息已持久化: {} ({} 条)", resolved_sid, agent_result.messages.len());
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "done",
                                    "data": {
                                        "session_id": resolved_sid,
                                        "content": agent_result.content,
                                        "iterations": agent_result.iterations,
                                    }
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = sender
                        .send(Message::Text(
                            serde_json::json!({
                                "type": "error",
                                "data": {"message": e.to_string()}
                            })
                            .to_string(),
                        ))
                        .await;
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
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "chunk",
                                    "data": step.content,
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                    _ => {
                        let _ = sender
                            .send(Message::Text(
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
                                })
                                .to_string(),
                            ))
                            .await;
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
                    // 检查 stop 指令
                    if text.contains("\"type\":\"stop\"") {
                        tracing::info!("用户请求停止生成");
                        cancel_flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);

                        // 回复前端停止确认
                        let mut sender = sender_for_cancel.lock().await;
                        let _ = sender
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "stopped",
                                    "data": {
                                        "reason": "user_cancel",
                                    }
                                })
                                .to_string(),
                            ))
                            .await;
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    // 等待对话完成
    let _ = agent_task.await;
    step_forward.abort();
    cancel_listener.abort();
}

pub fn routes() -> Router {
    Router::new().route("/chat", get(ws_chat_handler))
}
