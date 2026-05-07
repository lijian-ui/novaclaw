use futures_util::StreamExt;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::types::*;
use crate::config::ProviderConfig;
use crate::error::AppError;

/// 对 base_url 做智能标准化：
/// - 去除末尾多余斜杠
/// - 如果 URL 路径中不含 `/v1`，自动追加 `/v1`（兼容 LM Studio / Ollama 等本地服务）
pub fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    // 如果已含 /v1/ 或以 /v1 结尾，直接返回
    if trimmed.ends_with("/v1") || trimmed.contains("/v1/") {
        return trimmed.to_string();
    }
    // 自动追加 /v1（Ollama / LM Studio 等兼容 OpenAI 的本地服务需要）
    if trimmed.contains("localhost") || trimmed.contains("127.0.0.1") {
        return format!("{}/v1", trimmed);
    }
    trimmed.to_string()
}

/// LLM API 客户端
#[derive(Debug, Clone)]
pub struct LlmClient {
    http: Client,
    provider: Arc<ProviderConfig>,
    timeout_secs: u32,
}

impl LlmClient {
    /// 创建新的 LLM 客户端
    pub fn new(provider: ProviderConfig, timeout_secs: u32) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs as u64 + 30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            provider: Arc::new(provider),
            timeout_secs,
        }
    }

    /// 获取标准化后的 API Base URL
    pub fn api_base(&self) -> String {
        normalize_base_url(&self.provider.base_url)
    }

    /// 非流式聊天
    pub async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, AppError> {
        let base = self.api_base();
        let url = format!("{}/chat/completions", base);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.provider.api_key))
            .json(req)
            .send()
            .await
            .map_err(|e| AppError::LlmError(format!("请求失败: {}", e)))?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            AppError::LlmError(format!("读取响应失败: {}", e))
        })?;

        if !status.is_success() {
            return Err(AppError::LlmError(format!(
                "API 返回错误 ({}): {}",
                status, body
            )));
        }

        // 先检查响应体是否包含非标准的 "error" 字段（LM Studio 等本地服务即使 200 也可能返回错误 JSON）
        if let Ok(err_val) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(err_msg) = err_val.get("error") {
                let msg = err_msg.as_str().unwrap_or("未知错误");
                return Err(AppError::LlmError(format!(
                    "服务端返回错误: {}\n\n提示: 请确认 base_url 配置正确，本地服务（LM Studio/Ollama）的 base_url 应为 http://localhost:XXXX/v1",
                    msg
                )));
            }
        }

        let chat_response = serde_json::from_str::<ChatResponse>(&body).map_err(|e| {
            AppError::LlmError(format!("解析响应失败: {} — body: {}", e, &body[..body.len().min(500)]))
        })?;

        Ok(chat_response)
    }

    /// 流式聊天 - 返回 SSE 事件流
    pub async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamEvent>, AppError> {
        let base = self.api_base();
        let url = format!("{}/chat/completions", base);
        let (tx, rx) = mpsc::channel(256);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.provider.api_key))
            .header("Accept", "text/event-stream")
            .json(req)
            .send()
            .await
            .map_err(|e| AppError::LlmError(format!("流式请求失败: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::LlmError(format!(
                "API 返回错误 ({}): {}",
                status, body
            )));
        }

        // 在独立任务中解析 SSE 流
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut tool_call_index = 0usize;
            let mut tool_call_id = String::new();
            let mut tool_call_name = String::new();
            let mut tool_call_args = String::new();

            async fn send_event(tx: &mpsc::Sender<StreamEvent>, event: StreamEvent) -> bool {
                tx.send(event).await.is_ok()
            }

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let chunk_str = String::from_utf8_lossy(&chunk);
                        buffer.push_str(&chunk_str);

                        while let Some(pos) = buffer.find("\n\n") {
                            let line_block = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            for line in line_block.lines() {
                                let line = line.trim();
                                if line.is_empty() || line == "data: [DONE]" {
                                    continue;
                                }

                                let data = if let Some(rest) = line.strip_prefix("data: ") {
                                    rest
                                } else {
                                    continue;
                                };

                                match serde_json::from_str::<ChatResponse>(data) {
                                    Ok(resp) => {
                                        for choice in &resp.choices {
                                            if let Some(ref delta) = choice.delta {
                                                // 文本内容增量
                                                if let Some(ref content) = delta.content {
                                                    if !content.is_empty() {
                                                        if !send_event(&tx, StreamEvent::TextDelta(content.clone())).await {
                                                            return;
                                                        }
                                                    }
                                                }

                                                // 推理内容增量 (CoT)
                                                if let Some(ref reasoning) = delta.reasoning_content {
                                                    if !reasoning.is_empty() {
                                                        if !send_event(&tx, StreamEvent::ReasoningDelta(reasoning.clone())).await {
                                                            return;
                                                        }
                                                    }
                                                }

                                                // 工具调用增量
                                                if let Some(ref tc_deltas) = delta.tool_calls {
                                                    for tc in tc_deltas {
                                                        if let Some(ref id) = tc.id {
                                                            tool_call_id = id.clone();
                                                            tool_call_index = tc.index as usize;
                                                        }
                                                        if let Some(ref func) = tc.function {
                                                            if let Some(ref name) = func.name {
                                                                tool_call_name = name.clone();
                                                            }
                                                            if let Some(ref args) = func.arguments {
                                                                tool_call_args.push_str(args);
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // 检查 finish_reason
                                            if choice.finish_reason.as_deref() == Some("tool_calls") {
                                                if !tool_call_id.is_empty() && !tool_call_name.is_empty() {
                                                    if !send_event(&tx, StreamEvent::ToolCallDelta {
                                                        index: tool_call_index,
                                                        id: std::mem::take(&mut tool_call_id),
                                                        name: std::mem::take(&mut tool_call_name),
                                                        arguments: std::mem::take(&mut tool_call_args),
                                                    }).await {
                                                        return;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => { /* 忽略非 JSON 行 */ }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(format!("流读取错误: {}", e))).await;
                        return;
                    }
                }
            }

            let _ = tx.send(StreamEvent::Done("done".to_string())).await;
        });

        Ok(rx)
    }
}
