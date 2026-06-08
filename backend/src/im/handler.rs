//! IM 入站消息处理器

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crate::dingtalk::frames::CallbackMessageData;
use crate::dingtalk::DingTalkClient;
use crate::im::types::{Attachment, ConversationType, IncomingMessage, PlatformType};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// IMGateway 回调处理器（钉钉消息 → IncomingMessage → IMGateway）
///
/// 若提供了 `client`，会自动处理图片/文件消息：
/// 从 `content.downloadCode` 下载二进制并转为 Base64 data URL，
/// 存入 `media_urls`，同时 `text` 设为描述文本。
pub struct IMGatewayCallbackHandler {
    incoming_tx: mpsc::UnboundedSender<IncomingMessage>,
    account_id: String,
    account_name: Option<String>,
    client: Option<Arc<DingTalkClient>>,
}

impl IMGatewayCallbackHandler {
    pub fn new(
        incoming_tx: mpsc::UnboundedSender<IncomingMessage>,
        account_id: String,
        account_name: Option<String>,
    ) -> Self {
        Self {
            incoming_tx,
            account_id,
            account_name,
            client: None,
        }
    }

    /// 设置 DingTalkClient，启用媒体文件自动下载
    pub fn with_client(mut self, client: Arc<DingTalkClient>) -> Self {
        self.client = Some(client);
        self
    }

    /// 从 msg.content 中提取 downloadCode/pictureDownloadCode 并下载为 Base64 data URL
    async fn download_picture(
        client: &Option<Arc<DingTalkClient>>,
        content: &Option<serde_json::Value>,
        media_urls: &mut Vec<String>,
    ) {
        let Some(ref client) = client else {
            tracing::warn!("[图片处理] client 未设置（未调用 with_client），跳过图片下载");
            return;
        };
        let Some(content) = content else {
            tracing::warn!("[图片处理] content 为 None");
            return;
        };

        tracing::info!(
            "[图片处理] content字段={:?}, 可用字段={:?}",
            content,
            content.as_object().map(|obj| obj.keys().collect::<Vec<_>>())
        );

        let code = content
            .get("downloadCode")
            .and_then(|v| v.as_str())
            .or_else(|| content.get("pictureDownloadCode").and_then(|v| v.as_str()));

        match code {
            Some(code) => {
                tracing::info!("[图片处理] 找到 downloadCode={}", code);
                match client.download_media_to_base64(code, "image/jpeg").await {
                    Ok(data_url) => {
                        tracing::info!(
                            "[图片处理] download_media_to_base64 成功, data_url前缀={}, 全长={}字符",
                            &data_url[..data_url.len().min(60)],
                            data_url.len()
                        );
                        media_urls.push(data_url);
                    }
                    Err(e) => {
                        tracing::warn!("[图片处理] 下载图片失败 (downloadCode={}): {}", code, e);
                    }
                }
            }
            None => {
                tracing::warn!(
                    "[图片处理] content 中没有 downloadCode 或 pictureDownloadCode 字段! 完整content={:?}",
                    content
                );
            }
        }
    }

    /// 检测文件扩展名是否为文本类型
    fn is_text_file_ext(file_name: &str) -> bool {
        let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
        matches!(
            ext.as_str(),
            "txt" | "md" | "json" | "csv" | "log" | "xml" | "yaml" | "yml"
                | "toml" | "ini" | "cfg" | "conf" | "sh" | "bat" | "ps1"
                | "py" | "js" | "ts" | "rs" | "go" | "java" | "c" | "cpp"
                | "h" | "hpp" | "sql" | "html" | "css" | "scss" | "less"
                | "yaml" | "yml" | "json" | "xml"
        )
    }

    /// 下载文件并返回文本描述和附件
    async fn download_file_attachment(
        client: &Option<Arc<DingTalkClient>>,
        content: &Option<serde_json::Value>,
    ) -> (String, Option<Attachment>) {
        let Some(ref client) = client else {
            return (String::new(), None);
        };
        let Some(content) = content else {
            return (String::new(), None);
        };

        let download_code = content.get("downloadCode").and_then(|v| v.as_str());
        let file_name = content.get("fileName").and_then(|v| v.as_str()).unwrap_or("未知文件");

        let Some(code) = download_code else {
            return (format!("[用户发送了一个文件: {}]", file_name), None);
        };

        // 获取下载 URL
        match client.download_file(code).await {
            Ok(download_url) => {
                // 下载文件内容
                let http_client = reqwest::Client::new();
                match http_client.get(&download_url).send().await {
                    Ok(resp) => {
                        let bytes = match resp.bytes().await {
                            Ok(b) => b,
                            Err(e) => return (format!("[文件下载失败: {}]", e), None),
                        };
                        let file_size = bytes.len();

                        // 创建附件
                        let attachment = Attachment {
                            file_name: file_name.to_string(),
                            data: bytes.to_vec(),
                            mime_type: "application/octet-stream".to_string(),
                        };

                        let text = format!("[用户发送了一个文件: {} ({} 字节)]", file_name, file_size);
                        (text, Some(attachment))
                    }
                    Err(e) => (format!("[文件下载失败: {}]", e), None),
                }
            }
            Err(e) => (format!("[文件获取下载链接失败: {}]", e), None),
        }
    }
}

#[async_trait]
impl crate::dingtalk::handler::CallbackHandler for IMGatewayCallbackHandler {
    async fn on_callback_message(
        &self,
        msg: CallbackMessageData,
        _session_webhook: Option<String>,
    ) {
        let mut media_urls: Vec<String> = Vec::new();
        let mut attachments: Vec<Attachment> = Vec::new();
        let mut video_data_urls: Vec<String> = Vec::new();
        let mut text = msg
            .text
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();

        // 根据不同消息类型处理文本和媒体内容
        match msg.msgtype.as_str() {
            "picture" => {
                text = "[用户发送了一张图片]".to_string();
                Self::download_picture(&self.client, &msg.content, &mut media_urls).await;
            }
            "richText" => {
                let mut text_buf = String::new();
                if let Some(content) = &msg.content {
                    if let Some(rich_text) = content.get("richText").and_then(|v| v.as_array()) {
                        tracing::info!(
                            "[richText处理] 共 {} 个片段",
                            rich_text.len()
                        );
                        for (idx, item) in rich_text.iter().enumerate() {
                            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            match item_type {
                                "text" => {
                                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                                        text_buf.push_str(t);
                                    }
                                }
                                "picture" => {
                                    // 逐个下载图片
                                    let download_code = item.get("downloadCode").and_then(|v| v.as_str());
                                    let pic_code = item.get("pictureDownloadCode").and_then(|v| v.as_str());
                                    let code = download_code.or(pic_code);
                                    if let Some(code) = code {
                                        tracing::info!("[richText处理] 片段[{}] 找到图片 downloadCode={}", idx, code);
                                        if let Some(ref client) = self.client {
                                            match client.download_media_to_base64(code, "image/jpeg").await {
                                                Ok(data_url) => {
                                                    tracing::info!("[richText处理] 片段[{}] 图片下载成功, data_url前缀={}",
                                                        idx, &data_url[..data_url.len().min(60)]);
                                                    media_urls.push(data_url);
                                                }
                                                Err(e) => {
                                                    tracing::warn!("[richText处理] 片段[{}] 图片下载失败: {}", idx, e);
                                                }
                                            }
                                        }
                                    }
                                }
                                other => {
                                    tracing::info!("[richText处理] 片段[{}] 未识别类型={}", idx, other);
                                }
                            }
                        }
                    } else {
                        tracing::warn!("[richText处理] content 中没有 richText 数组, content={:?}", content);
                    }
                } else {
                    tracing::warn!("[richText处理] msgtype=richText 但 content 为 None");
                }
                if text_buf.is_empty() {
                    text = "[用户发送了图文消息]".to_string();
                } else {
                    text = text_buf;
                }
            }
            "file" => {
                tracing::info!("[文件处理] msgtype=file, content={:?}", msg.content);
                // 检测是否为视频文件（钉钉发送 .mp4 等视频文件走 file 类型而非 video 类型）
                let is_video = msg.content
                    .as_ref()
                    .and_then(|c| c.get("fileName"))
                    .and_then(|n| n.as_str())
                    .map(|name| {
                        let lower = name.to_lowercase();
                        lower.ends_with(".mp4")
                            || lower.ends_with(".mov")
                            || lower.ends_with(".avi")
                            || lower.ends_with(".mkv")
                            || lower.ends_with(".webm")
                            || lower.ends_with(".wmv")
                            || lower.ends_with(".flv")
                    })
                    .unwrap_or(false);

                if is_video {
                    // 视频文件：下载并转 base64 给多模态 LLM
                    let file_name = msg.content
                        .as_ref()
                        .and_then(|c| c.get("fileName"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("video.mp4");
                    let ext = file_name.rsplit('.').next().unwrap_or("mp4");
                    let download_code = msg.content
                        .as_ref()
                        .and_then(|c| c.get("downloadCode"))
                        .and_then(|v| v.as_str());

                    if let (Some(ref client), Some(code)) = (&self.client, download_code) {
                        match client.download_file(code).await {
                            Ok(download_url) => {
                                let http_client = reqwest::Client::new();
                                match http_client.get(&download_url).send().await {
                                    Ok(resp) => {
                                        if let Ok(bytes) = resp.bytes().await {
                                            let raw_len = bytes.len();
                                            text = format!("[用户发送了一个视频 ({}KB)]", raw_len / 1024);
                                            // 小米全模态限制 base64 ≤ 50MB，原始文件需 ≤ ~37MB
                                            if raw_len <= 37 * 1024 * 1024 {
                                                let b64 = STANDARD.encode(&bytes);
                                                let data_url = format!("data:video/{};base64,{}", ext, b64);
                                                video_data_urls.push(data_url);
                                                tracing::info!("[文件处理] 视频已转换为 base64, 原始大小={}MB", raw_len / 1024 / 1024);
                                            } else {
                                                tracing::warn!("[文件处理] 视频过大 ({}MB > 37MB)，跳过 base64 编码", raw_len / 1024 / 1024);
                                            }
                                            attachments.push(Attachment {
                                                file_name: file_name.to_string(),
                                                data: bytes.to_vec(),
                                                mime_type: format!("video/{}", ext),
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        text = format!("[视频下载失败: {}]", e);
                                    }
                                }
                            }
                            Err(e) => {
                                text = format!("[视频获取下载链接失败: {}]", e);
                            }
                        }
                    } else {
                        text = format!("[用户发送了一个视频: {}]", file_name);
                    }
                } else {
                    // 普通文件：下载并保存为附件
                    let (desc, attachment) = Self::download_file_attachment(&self.client, &msg.content).await;
                    text = desc;
                    if let Some(att) = attachment {
                        attachments.push(att);
                    }
                }
            }
            "video" => {
                tracing::info!("[视频处理] msgtype=video, content={:?}", msg.content);
                if let Some(content) = &msg.content {
                    let duration = content.get("duration").and_then(|v| v.as_str()).unwrap_or("未知");
                    let video_type = content.get("videoType").and_then(|v| v.as_str()).unwrap_or("未知格式");
                    text = format!("[用户发送了一个视频 (时长: {}s, 格式: {})]", duration, video_type);

                    // 尝试下载视频文件作为附件，并转换为 base64 供多模态 LLM 使用
                    let download_code = content.get("downloadCode").and_then(|v| v.as_str());
                    if let (Some(ref client), Some(code)) = (&self.client, download_code) {
                        match client.download_file(code).await {
                            Ok(download_url) => {
                                let http_client = reqwest::Client::new();
                                if let Ok(resp) = http_client.get(&download_url).send().await {
                                    if let Ok(bytes) = resp.bytes().await {
                                        let video_name = format!("video_{}.{}", Uuid::new_v4(), video_type);
                                        let raw_len = bytes.len();
                                        // 小米全模态限制 base64 ≤ 50MB，原始文件需 ≤ ~37MB
                                        if raw_len <= 37 * 1024 * 1024 {
                                            let b64 = STANDARD.encode(&bytes);
                                            let data_url = format!("data:video/{};base64,{}", video_type, b64);
                                            video_data_urls.push(data_url);
                                            tracing::info!("[视频处理] 已转换为 base64, 原始大小={}MB", raw_len / 1024 / 1024);
                                        } else {
                                            tracing::warn!("[视频处理] 视频过大 ({}MB > 37MB)，跳过 base64 编码", raw_len / 1024 / 1024);
                                        }
                                        attachments.push(Attachment {
                                            file_name: video_name,
                                            data: bytes.to_vec(),
                                            mime_type: format!("video/{}", video_type),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("[视频处理] 下载视频失败: {}", e);
                            }
                        }
                    }
                } else {
                    text = "[用户发送了一个视频]".to_string();
                }
            }
            "voice" => {
                tracing::info!("[语音处理] msgtype=voice, content={:?}", msg.content);
                // 优先使用识别文字
                if let Some(content) = &msg.content {
                    if let Some(recognition) = content.get("recognition").and_then(|v| v.as_str()) {
                        let recognition = recognition.trim();
                        if !recognition.is_empty() {
                            text = format!("[用户发送了一条语音, 转文字: {}]", recognition);
                        } else {
                            text = "[用户发送了一条语音]".to_string();
                        }
                    } else {
                        let duration = content.get("duration").and_then(|v| v.as_str()).unwrap_or("");
                        text = if !duration.is_empty() {
                            format!("[用户发送了一条语音 (时长: {}s)]", duration)
                        } else {
                            "[用户发送了一条语音]".to_string()
                        };
                    }
                } else {
                    text = "[用户发送了一条语音]".to_string();
                }
            }
            _ => {
                if text.is_empty() {
                    tracing::info!("[消息处理] 未识别的 msgtype={}", msg.msgtype);
                } else {
                    tracing::info!("[消息处理] msgtype={} 有文本内容: {}", msg.msgtype, text.chars().take(80).collect::<String>());
                }
            }
        }

        // 记录最终 media_urls（仅 picture/richText 类型需要关注）
        let is_media_type = msg.msgtype == "picture" || msg.msgtype == "richText";
        if is_media_type && media_urls.is_empty() {
            tracing::warn!("[图片处理] 最终 media_urls 为空, msgtype={}", msg.msgtype);
        } else if is_media_type && !media_urls.is_empty() {
            tracing::info!("[图片处理] 最终 media_urls 有 {} 条", media_urls.len());
        }

        let incoming_msg = IncomingMessage {
            id: msg.msg_id.clone().unwrap_or_default(),
            account_id: self.account_id.clone(),
            account_name: self.account_name.clone(),
            platform: PlatformType::DingTalk,
            conversation_id: msg
                .conversation_id
                .clone()
                .unwrap_or_else(|| msg.sender_id.clone().unwrap_or_default()),
            sender_id: msg.sender_id.clone(),
            sender_staff_id: msg.sender_staff_id.clone(),
            sender_name: msg.sender_nick.clone(),
            text,
            media_urls,
            video_data_urls,
            attachments,
            raw: serde_json::to_value(&msg).unwrap_or_default(),
            session_webhook: msg.session_webhook.clone(),
            conversation_type: msg
                .conversation_type
                .as_deref()
                .map(ConversationType::from_dingtalk)
                .unwrap_or(ConversationType::Private),
            conversation_title: msg.conversation_title.clone(),
            timestamp: msg.create_at.unwrap_or(0),
        };

        if let Err(e) = self.incoming_tx.send(incoming_msg) {
            tracing::error!("发送入站消息到 IMGateway 失败: {}", e);
        }
    }
}
