//! 通过钉钉 REST API 发送消息

use crate::dingtalk::credential::TokenManager;
use std::sync::Arc;
use crate::dingtalk::frames::{
    FileDownloadRequest, FileDownloadResponse, GroupMessageRequest, MediaUploadResponse,
    PrivateMessageRequest, WebhookReply, MSG_KEY_MARKDOWN, MSG_KEY_TEXT,
};
use crate::error::AppError;

/// 消息发送器
pub struct MessageSender {
    http_client: reqwest::Client,
    token_manager: Arc<TokenManager>,
    robot_code: String,
}

impl MessageSender {
    pub fn new(
        http_client: reqwest::Client,
        token_manager: Arc<TokenManager>,
        robot_code: String,
    ) -> Self {
        Self {
            http_client,
            token_manager,
            robot_code,
        }
    }

    /// 获取底层 TokenManager 引用
    fn token_mgr(&self) -> &TokenManager {
        &self.token_manager
    }

    /// 发送私聊消息
    pub async fn send_private_message(
        &self,
        user_ids: Vec<String>,
        content: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend";

        let request = PrivateMessageRequest {
            robot_code: self.robot_code.clone(),
            user_ids,
            msg_param: serde_json::json!({"content": content}).to_string(),
            msg_key: MSG_KEY_TEXT.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉发送私聊消息失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉发送私聊消息失败 (HTTP {}): {}",
                status, text
            )));
        }

        Ok(())
    }

    /// 发送群聊消息
    pub async fn send_group_message(
        &self,
        open_conversation_id: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/groupMessages/send";

        let request = GroupMessageRequest {
            robot_code: self.robot_code.clone(),
            open_conversation_id: open_conversation_id.to_string(),
            msg_param: serde_json::json!({"content": content}).to_string(),
            msg_key: MSG_KEY_TEXT.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉发送群聊消息失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉发送群聊消息失败 (HTTP {}): {}",
                status, text
            )));
        }

        Ok(())
    }

    /// 发送 Markdown 私聊消息
    pub async fn send_private_markdown(
        &self,
        user_ids: Vec<String>,
        title: &str,
        text: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend";

        let request = PrivateMessageRequest {
            robot_code: self.robot_code.clone(),
            user_ids,
            msg_param: serde_json::json!({"title": title, "text": text}).to_string(),
            msg_key: MSG_KEY_MARKDOWN.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉发送私聊 Markdown 失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉发送私聊 Markdown 失败 (HTTP {}): {}",
                status, text
            )));
        }

        Ok(())
    }

    /// 发送 Markdown 群聊消息
    pub async fn send_group_markdown(
        &self,
        open_conversation_id: &str,
        title: &str,
        text: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/groupMessages/send";

        let request = GroupMessageRequest {
            robot_code: self.robot_code.clone(),
            open_conversation_id: open_conversation_id.to_string(),
            msg_param: serde_json::json!({"title": title, "text": text}).to_string(),
            msg_key: MSG_KEY_MARKDOWN.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉发送群聊 Markdown 失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉发送群聊 Markdown 失败 (HTTP {}): {}",
                status, text
            )));
        }

        Ok(())
    }

    /// 通过会话 Webhook 回复消息
    pub async fn reply_via_webhook(
        &self,
        webhook_url: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let reply = WebhookReply::text(content);

        let resp = self
            .http_client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&reply)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉 Webhook 回复失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉 Webhook 回复失败 (HTTP {}): {}", status, text
            )));
        }

        Ok(())
    }

    /// 通过会话 Webhook 回复 Markdown 消息
    pub async fn reply_markdown_via_webhook(
        &self,
        webhook_url: &str,
        title: &str,
        text: &str,
    ) -> Result<(), AppError> {
        let reply = WebhookReply::markdown(title, text);

        let resp = self
            .http_client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&reply)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉 Webhook Markdown 回复失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉 Webhook Markdown 回复失败 (HTTP {}): {}", status, text
            )));
        }

        Ok(())
    }

    /// 下载消息中的文件
    pub async fn download_file(
        &self,
        download_code: &str,
    ) -> Result<String, AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/messageFiles/download";

        let request = FileDownloadRequest {
            robot_code: self.robot_code.clone(),
            download_code: download_code.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉文件下载请求失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉文件下载请求失败 (HTTP {}): {}",
                status, text
            )));
        }

        let download_resp: FileDownloadResponse = resp
            .json()
            .await
            .map_err(|e| AppError::External(format!("钉钉解析文件下载响应失败: {}", e)))?;

        Ok(download_resp.download_url)
    }

    /// 上传媒体文件
    pub async fn upload_media(
        &self,
        _media_type: &str,
        _file_data: Vec<u8>,
        _file_name: &str,
    ) -> Result<MediaUploadResponse, AppError> {
        Err(AppError::External("媒体上传功能暂未启用（缺少 reqwest multipart feature）".to_string()))
    }
}
