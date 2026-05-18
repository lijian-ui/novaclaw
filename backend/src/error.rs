use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// 应用错误类型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("配置错误: {0}")]
    Config(String),

    #[error("未找到: {0}")]
    NotFound(String),

    #[error("参数无效: {0}")]
    BadRequest(String),

    #[error("内部错误: {0}")]
    Internal(String),

    #[error("存储错误: {0}")]
    Storage(String),

    #[error("LLM 错误: {0}")]
    LlmError(String),

    #[error("工具错误: {0}")]
    ToolError(String),

    #[error("Agent 错误: {0}")]
    AgentError(String),

    #[error("外部服务错误: {0}")]
    External(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Config(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("配置错误: {}", msg)),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Storage(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("存储错误: {}", msg)),
            AppError::LlmError(msg) => (StatusCode::BAD_GATEWAY, format!("LLM 错误: {}", msg)),
            AppError::ToolError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("工具错误: {}", msg)),
            AppError::AgentError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Agent 错误: {}", msg)),
            AppError::External(msg) => (StatusCode::BAD_GATEWAY, format!("外部服务错误: {}", msg)),
            AppError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("IO 错误: {}", e)),
            AppError::Serde(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("序列化错误: {}", e)),
            AppError::Anyhow(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("错误: {}", e)),
        };

        let body = Json(json!({
            "success": false,
            "message": message,
        }));

        (status, body).into_response()
    }
}
