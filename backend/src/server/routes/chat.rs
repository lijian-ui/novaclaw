use axum::{routing::post, Json, Router};
use serde::Deserialize;
use crate::APP_STATE;

#[derive(Deserialize)]
struct ChatRequest {
    #[serde(default)]
    session_id: Option<String>,
    message: String,
    #[serde(default)]
    model: Option<String>,
    stream: bool,
}

#[derive(Deserialize)]
struct TestConnectionReq {
    api_key: String,
    base_url: String,
    model: String,
}

/// 非流式聊天
async fn chat(Json(req): Json<ChatRequest>) -> Json<serde_json::Value> {
    let state = APP_STATE.read().await;

    // 获取或创建会话
    let session_id = match &req.session_id {
        Some(id) => id.clone(),
        None => {
            let session_name = &req.message[..req.message.len().min(50)];
            match state.session_store.create_session(session_name, req.model.as_deref()) {
                Ok(session) => session.id,
                Err(e) => return Json(serde_json::json!({ "success": false, "message": e.to_string() })),
            }
        }
    };

    // 构建 Agent 运行时
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

    let llm_client = crate::llm::client::LlmClient::new(
        provider,
        state.config.llm_timeout,
    );

    let agent_session = crate::agent::AgentSession::new(
        &format!("chat-{}", &session_id[..8]),
        model,
        None,
    );

    let tool_registry = state.tool_registry.clone();
    let config = state.config.clone();
    drop(state);

    let mut runtime = crate::agent::AgentRuntime::new(
        agent_session,
        llm_client,
        std::sync::Arc::new(tool_registry),
        &config,
    );

    // 执行非流式对话
    match runtime.run_turn(&req.message, None).await {
        Ok(result) => {
            // 保存消息到会话
            let state = APP_STATE.read().await;
            let _ = state.session_store.append_message(
                &session_id,
                &crate::storage::Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session_id.clone(),
                    role: "assistant".to_string(),
                    content: result.content.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    metadata: None,
                },
            );

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

/// 测试提供商连接
async fn test_connection(Json(req): Json<TestConnectionReq>) -> Json<serde_json::Value> {
    // 对 base_url 做智能标准化（自动追加 /v1 后缀，兼容 LM Studio/Ollama 等本地服务）
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
            // 如果是 base_url 自动修正的情况，给出明确的诊断提示
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
        .route("/chat/test", post(test_connection))
}
