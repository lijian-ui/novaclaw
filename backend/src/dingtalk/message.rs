//! 通过钉钉 REST API 发送消息

use crate::dingtalk::credential::TokenManager;
use std::sync::Arc;
use crate::dingtalk::frames::{
    AudioMsgParam, FileDownloadRequest, FileDownloadResponse, FileMsgParam, GroupMessageRequest,
    MediaUploadResponse, PrivateMessageRequest, VideoMsgParam, WebhookAtConfig, WebhookReply,
    MSG_KEY_AUDIO, MSG_KEY_FILE, MSG_KEY_IMAGE, MSG_KEY_MARKDOWN, MSG_KEY_TEXT, MSG_KEY_VIDEO,
};
use crate::error::AppError;
use base64::Engine;

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

    /// 下载消息中的媒体文件并转为 Base64 data URL
    ///
    /// 1. 用 downloadCode 换取下载 URL
    /// 2. 下载二进制内容
    /// 3. 转 Base64 并包装为 data URL
    pub async fn download_media_to_base64(
        &self,
        download_code: &str,
        mime_type: &str,
    ) -> Result<String, AppError> {
        // 1. 获取下载 URL
        let download_url = self.download_file(download_code).await?;

        // 2. 下载二进制内容
        let resp = self
            .http_client
            .get(&download_url)
            .send()
            .await
            .map_err(|e| AppError::External(format!("下载媒体文件失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "下载媒体文件失败 (HTTP {}): {}",
                status, text
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::External(format!("读取媒体文件响应体失败: {}", e)))?;

        // 3. 转 Base64
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime_type, b64);

        Ok(data_url)
    }

    /// 上传媒体文件到钉钉 OAPI
    /// API: POST https://oapi.dingtalk.com/media/upload?access_token=<token>&type=<media_type>
    ///
    /// media_type 可选值: image / file / voice / video
    pub async fn upload_media(
        &self,
        media_type: &str,
        file_data: Vec<u8>,
        file_name: &str,
    ) -> Result<MediaUploadResponse, AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://oapi.dingtalk.com/media/upload";

        // 根据媒体类型设置 MIME
        let mime = match media_type {
            "image" => "image/jpeg",
            "video" => "video/mp4",
            "voice" => "audio/amr",
            _ => "application/octet-stream",
        };

        let form = reqwest::multipart::Form::new()
            .text("type", media_type.to_string())
            .part(
                "media",
                reqwest::multipart::Part::bytes(file_data)
                    .file_name(file_name.to_string())
                    .mime_str(mime)
                    .map_err(|e| AppError::Internal(e.to_string()))?,
            );

        let resp = self
            .http_client
            .post(url)
            .query(&[("access_token", &token)])
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉媒体上传失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉媒体上传失败 (HTTP {}): {}",
                status, text
            )));
        }

        let upload_resp: MediaUploadResponse = resp
            .json()
            .await
            .map_err(|e| AppError::External(format!("钉钉解析媒体上传响应失败: {}", e)))?;

        if upload_resp.errcode != 0 {
            return Err(AppError::External(format!(
                "钉钉媒体上传业务错误 ({}): {}",
                upload_resp.errcode, upload_resp.errmsg
            )));
        }

        Ok(upload_resp)
    }

    /// 发送私聊图片消息
    pub async fn send_private_image(
        &self,
        user_ids: Vec<String>,
        photo_url: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend";

        let request = PrivateMessageRequest {
            robot_code: self.robot_code.clone(),
            user_ids,
            msg_param: serde_json::json!({"photoURL": photo_url}).to_string(),
            msg_key: MSG_KEY_IMAGE.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉发送私聊图片失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉发送私聊图片失败 (HTTP {}): {}",
                status, text
            )));
        }
        Ok(())
    }

    /// 发送群聊图片消息
    pub async fn send_group_image(
        &self,
        open_conversation_id: &str,
        photo_url: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/groupMessages/send";

        let request = GroupMessageRequest {
            robot_code: self.robot_code.clone(),
            open_conversation_id: open_conversation_id.to_string(),
            msg_param: serde_json::json!({"photoURL": photo_url}).to_string(),
            msg_key: MSG_KEY_IMAGE.to_string(),
        };

        let resp = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-acs-dingtalk-access-token", &token)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉发送群聊图片失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉发送群聊图片失败 (HTTP {}): {}",
                status, text
            )));
        }
        Ok(())
    }

    /// 下载远程文件到临时文件，返回(临时文件路径, 文件字节)
    async fn download_remote_file(&self, url: &str) -> Result<(String, Vec<u8>), AppError> {
        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::External(format!("下载远程文件失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(AppError::External(format!(
                "下载远程文件返回 HTTP {}",
                resp.status()
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::External(format!("读取远程文件响应体失败: {}", e)))?;

        // 写入临时文件
        let temp_dir = std::env::temp_dir();
        let ext = url
            .split('?')
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or("bin");
        let file_name = format!("dingtalk_file_{}.{}", uuid::Uuid::new_v4(), ext);
        let temp_path = temp_dir.join(file_name);

        tokio::fs::write(&temp_path, &bytes)
            .await
            .map_err(|e| AppError::External(format!("写入临时文件失败: {}", e)))?;

        Ok((temp_path.to_string_lossy().to_string(), bytes.to_vec()))
    }

    /// 发送私聊文件消息
    pub async fn send_private_file(
        &self,
        user_ids: Vec<String>,
        url: &str,
        file_name: &str,
    ) -> Result<(), AppError> {
        // 1. 下载远程文件到临时文件
        let (temp_path, file_data) = self.download_remote_file(url).await?;

        // 2. 上传到钉钉 OAPI media
        let upload_resp = self.upload_media("file", file_data, file_name).await?;
        let media_id = upload_resp.media_id.unwrap_or_default();

        // 3. 发送文件消息
        let result = self.send_private_msg_with_param(
            user_ids,
            serde_json::json!({"mediaId": media_id, "fileName": file_name}).to_string(),
            MSG_KEY_FILE,
        )
        .await;

        // 4. 清理临时文件
        let _ = tokio::fs::remove_file(&temp_path).await;

        result
    }

    /// 发送群聊文件消息
    pub async fn send_group_file(
        &self,
        open_conversation_id: &str,
        url: &str,
        file_name: &str,
    ) -> Result<(), AppError> {
        // 1. 下载远程文件到临时文件
        let (temp_path, file_data) = self.download_remote_file(url).await?;

        // 2. 上传到钉钉 OAPI media
        let upload_resp = self.upload_media("file", file_data, file_name).await?;
        let media_id = upload_resp.media_id.unwrap_or_default();

        // 3. 发送文件消息
        let result = self.send_group_msg_with_param(
            open_conversation_id,
            serde_json::json!({"mediaId": media_id, "fileName": file_name}).to_string(),
            MSG_KEY_FILE,
        )
        .await;

        // 4. 清理临时文件
        let _ = tokio::fs::remove_file(&temp_path).await;

        result
    }

    /// 发送私聊视频消息
    pub async fn send_private_video(
        &self,
        user_ids: Vec<String>,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        // 1. 下载远程视频到临时文件
        let file_name = "video.mp4";
        let (temp_path, file_data) = self.download_remote_file(url).await?;

        // 2. 上传到钉钉 OAPI media
        let upload_resp = self.upload_media("video", file_data, file_name).await?;
        let media_id = upload_resp.media_id.unwrap_or_default();

        // 3. 发送视频消息
        let result = self.send_private_msg_with_param(
            user_ids,
            serde_json::json!({"videoMediaId": media_id, "videoType": "mp4", "duration": duration}).to_string(),
            MSG_KEY_VIDEO,
        )
        .await;

        // 4. 清理临时文件
        let _ = tokio::fs::remove_file(&temp_path).await;

        result
    }

    /// 发送群聊视频消息
    pub async fn send_group_video(
        &self,
        open_conversation_id: &str,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        let file_name = "video.mp4";
        let (temp_path, file_data) = self.download_remote_file(url).await?;

        let upload_resp = self.upload_media("video", file_data, file_name).await?;
        let media_id = upload_resp.media_id.unwrap_or_default();

        let result = self.send_group_msg_with_param(
            open_conversation_id,
            serde_json::json!({"videoMediaId": media_id, "videoType": "mp4", "duration": duration}).to_string(),
            MSG_KEY_VIDEO,
        )
        .await;

        let _ = tokio::fs::remove_file(&temp_path).await;

        result
    }

    /// 发送私聊音频消息
    pub async fn send_private_audio(
        &self,
        user_ids: Vec<String>,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        let file_name = "audio.amr";
        let (temp_path, file_data) = self.download_remote_file(url).await?;

        let upload_resp = self.upload_media("voice", file_data, file_name).await?;
        let media_id = upload_resp.media_id.unwrap_or_default();

        let result = self.send_private_msg_with_param(
            user_ids,
            serde_json::json!({"audioMediaId": media_id, "duration": duration}).to_string(),
            MSG_KEY_AUDIO,
        )
        .await;

        let _ = tokio::fs::remove_file(&temp_path).await;

        result
    }

    /// 发送群聊音频消息
    pub async fn send_group_audio(
        &self,
        open_conversation_id: &str,
        url: &str,
        duration: i64,
    ) -> Result<(), AppError> {
        let file_name = "audio.amr";
        let (temp_path, file_data) = self.download_remote_file(url).await?;

        let upload_resp = self.upload_media("voice", file_data, file_name).await?;
        let media_id = upload_resp.media_id.unwrap_or_default();

        let result = self.send_group_msg_with_param(
            open_conversation_id,
            serde_json::json!({"audioMediaId": media_id, "duration": duration}).to_string(),
            MSG_KEY_AUDIO,
        )
        .await;

        let _ = tokio::fs::remove_file(&temp_path).await;

        result
    }

    /// 通用私聊消息发送（内部使用）
    async fn send_private_msg_with_param(
        &self,
        user_ids: Vec<String>,
        msg_param: String,
        msg_key: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend";

        let request = PrivateMessageRequest {
            robot_code: self.robot_code.clone(),
            user_ids,
            msg_param,
            msg_key: msg_key.to_string(),
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

    /// 通用群聊消息发送（内部使用）
    async fn send_group_msg_with_param(
        &self,
        open_conversation_id: &str,
        msg_param: String,
        msg_key: &str,
    ) -> Result<(), AppError> {
        let token = self.token_manager.get_token().await?;
        let url = "https://api.dingtalk.com/v1.0/robot/groupMessages/send";

        let request = GroupMessageRequest {
            robot_code: self.robot_code.clone(),
            open_conversation_id: open_conversation_id.to_string(),
            msg_param,
            msg_key: msg_key.to_string(),
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

    /// 通过会话 Webhook 回复文本消息（带 @ 某人）
    pub async fn reply_with_at(
        &self,
        webhook_url: &str,
        content: &str,
        at_user_ids: Vec<String>,
    ) -> Result<(), AppError> {
        let at_config = WebhookAtConfig {
            at_user_ids: Some(at_user_ids),
            ..Default::default()
        };
        let reply = WebhookReply::text_with_at(content, &at_config);

        let resp = self
            .http_client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&reply)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉 Webhook @回复失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉 Webhook @回复失败 (HTTP {}): {}",
                status, text
            )));
        }
        Ok(())
    }

    /// 通过会话 Webhook 回复 Markdown 消息（带 @ 某人）
    pub async fn reply_markdown_with_at(
        &self,
        webhook_url: &str,
        title: &str,
        text: &str,
        at_user_ids: Vec<String>,
    ) -> Result<(), AppError> {
        let at_config = WebhookAtConfig {
            at_user_ids: Some(at_user_ids),
            ..Default::default()
        };
        let reply = WebhookReply::markdown_with_at(title, text, &at_config);

        let resp = self
            .http_client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&reply)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉 Webhook Markdown @回复失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉 Webhook Markdown @回复失败 (HTTP {}): {}",
                status, text
            )));
        }
        Ok(())
    }
}
