use futures_util::StreamExt;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::types::*;
use crate::config::ProviderConfig;
use crate::error::AppError;

/// 流式聊天返回的句柄，同时持有事件接收器和底层的 HTTP 流任务
/// 取消时通过 abort 底层任务强制关闭 HTTP 连接，让 LLM 服务真正停止生成
pub struct StreamHandle {
    pub rx: mpsc::Receiver<StreamEvent>,
    task: tokio::task::JoinHandle<()>,
}

impl StreamHandle {
    /// 立即终止流式请求（强制关闭 HTTP 连接）
    pub fn abort(&self) {
        self.task.abort();
    }
}

/// 对 base_url 做智能标准化：
/// - 去除末尾多余斜杠
/// - 如果 URL 路径中不含 `/v1`，自动追加 `/v1`（兼容 OpenAI 格式）
pub fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    // 如果已含 /v1 或 /v4（智谱）等版本路径，直接返回
    if trimmed.ends_with("/v1") || trimmed.contains("/v1/")
        || trimmed.ends_with("/v4") || trimmed.contains("/v4/")
    {
        return trimmed.to_string();
    }
    // 自动追加 /v1
    format!("{}/v1", trimmed)
}

/// LLM API 客户端
#[derive(Debug, Clone)]
pub struct LlmClient {
    http: Client,
    provider: Arc<ProviderConfig>,
    #[allow(dead_code)]
    timeout_secs: u32,
}

impl LlmClient {
    /// 创建新的 LLM 客户端
    pub fn new(provider: ProviderConfig, timeout_secs: u32) -> Result<Self, AppError> {
        let http = Client::builder()
            // 连接超时：连接不上时快速失败，不等完整 timeout
            .connect_timeout(std::time::Duration::from_secs(10))
            // 请求总超时（含连接、发送、等待响应）
            .timeout(std::time::Duration::from_secs(timeout_secs as u64))
            .build()
            .map_err(|e| AppError::Internal(format!("创建 HTTP 客户端失败: {}", e)))?;

        Ok(Self {
            http,
            provider: Arc::new(provider),
            timeout_secs,
        })
    }

    /// 获取标准化后的 API Base URL
    pub fn api_base(&self) -> String {
        normalize_base_url(&self.provider.base_url)
    }

    /// 根据提供商设置认证头
    fn auth_header(&self) -> (&'static str, String) {
        let name = self.provider.name.to_lowercase();
        if name.contains("anthropic") {
            ("x-api-key", self.provider.api_key.clone())
        } else {
            ("Authorization", format!("Bearer {}", self.provider.api_key))
        }
    }

    /// 非流式聊天
    pub async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, AppError> {
        let base = self.api_base();
        let url = format!("{}/chat/completions", base);
        let (header_name, header_value) = self.auth_header();

        let response = self
            .http
            .post(&url)
            .header(header_name, header_value)
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
            AppError::LlmError(format!("解析响应失败: {} — body: {}", e, body.chars().take(500).collect::<String>()))
        })?;

        Ok(chat_response)
    }

/// 流式聊天 - 返回 SSE 事件流
    /// cancel: 可选取消信号，收到取消时立即中止 SSE 读取
    pub async fn chat_stream(
        &self,
        req: &ChatRequest,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<StreamHandle, AppError> {
        let base = self.api_base();
        let url = format!("{}/chat/completions", base);
        let (tx, rx) = mpsc::channel(256);

        let (header_name, header_value) = self.auth_header();

        let response = self
            .http
            .post(&url)
            .header(header_name, header_value)
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

        let cancel_check = cancel.clone();
        let status_code = status.as_u16();

        // 在独立任务中解析 SSE 流
        let task = tokio::spawn(async move {
                let _response = response;
                let mut stream = _response.bytes_stream();
                let mut buffer = String::new();
                let mut tool_calls_acc: std::collections::HashMap<usize, (String, String, String)> = std::collections::HashMap::new();
                let mut early_sent_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

                async fn send_event(tx: &mpsc::Sender<StreamEvent>, event: StreamEvent) -> bool {
                    tx.send(event).await.is_ok()
                }

                'stream_loop: while let Some(chunk_result) = stream.next().await {
                    // 检查取消信号
                    if let Some(ref flag) = cancel_check {
                        if flag.load(std::sync::atomic::Ordering::Relaxed) {
                            break 'stream_loop;
                        }
                    }

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
                                                        let idx = tc.index as usize;
                                                        let entry = tool_calls_acc.entry(idx).or_insert_with(|| (String::new(), String::new(), String::new()));
                                                        if let Some(ref id) = tc.id {
                                                            entry.0 = id.clone();
                                                        }
                                                        if let Some(ref func) = tc.function {
                                                            if let Some(ref name) = func.name {
                                                                entry.1 = name.clone();
                                                                // 尽早发送：当工具名称已知时立即推送给 runtime
                                                                if !entry.0.is_empty() && !entry.1.is_empty() && !early_sent_indices.contains(&idx) {
                                                                    early_sent_indices.insert(idx);
                                                                    if !send_event(&tx, StreamEvent::ToolCallDelta {
                                                                        index: idx,
                                                                        id: entry.0.clone(),
                                                                        name: entry.1.clone(),
                                                                        arguments: String::new(), // 参数可能不完整，稍后更新
                                                                    }).await {
                                                                        return;
                                                                    }
                                                                }
                                                            }
                                                            if let Some(ref args) = func.arguments {
                                                                entry.2.push_str(args);
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                            // 检查 finish_reason
                                            if choice.finish_reason.as_deref() == Some("tool_calls") {
                                                // 发送所有累积的完整工具调用数据
                                                for (idx, (tid, tname, targs)) in tool_calls_acc.drain() {
                                                    if !tid.is_empty() && !tname.is_empty() {
                                                        if !send_event(&tx, StreamEvent::ToolCallDelta {
                                                            index: idx,
                                                            id: tid,
                                                            name: tname,
                                                            arguments: targs,
                                                        }).await {
                                                            return;
                                                        }
                                                    }
                                                }
                                                early_sent_indices.clear();
                                            }
                                        }

                                        // 提取 usage（部分提供商在流式响应中携带）
                                        if let Some(ref usage) = resp.usage {
                                            let pt = usage.prompt_tokens.unwrap_or(0) as u64;
                                            let ct = usage.completion_tokens.unwrap_or(0) as u64;
                                            // 优先使用 prompt_cache_hit_tokens（DeepSeek 精确字段），
                                            // 降级到 cached_tokens（通用兼容字段）
                                            let cached = usage.prompt_cache_hit_tokens
                                                .or(usage.cached_tokens)
                                                .unwrap_or(0) as u64;
                                            if pt > 0 || ct > 0 || cached > 0 {
                                                if !send_event(&tx, StreamEvent::Usage {
                                                    prompt_tokens: pt,
                                                    completion_tokens: ct,
                                                    cached_tokens: cached,
                                                }).await {
                                                    return;
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
                        let _ = tx.send(StreamEvent::Error(format!("流读取错误 (HTTP {status_code}): {}", e))).await;
                        return;
                    }
                }
            }

            let _ = tx.send(StreamEvent::Done("done".to_string())).await;
        });

        Ok(StreamHandle { rx, task })
    }
}
