//! 微信 IM 适配器
//!
//! 实现 IMAdapter trait，将微信 iLink 协议接入 Jeeves IM 系统。

use crate::error::AppError;
use crate::im::adapter::IMAdapter;
use crate::im::types::{
    Attachment, ConversationType, IncomingMessage, MessageTarget, PlatformCapabilities,
    PlatformType, SendResult,
};
use crate::weixin::client::{msg_item_type, WeixinClient};
use crate::weixin::upload;
use async_trait::async_trait;
use base64::Engine;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// 微信 IM 适配器
pub struct WeixinAdapter {
    pub client: Arc<WeixinClient>,
    pub account_id: String,
    pub account_name: Option<String>,
}

impl WeixinAdapter {
    pub fn new(client: Arc<WeixinClient>, account_id: String, account_name: Option<String>) -> Self {
        Self { client, account_id, account_name }
    }

    /// 启动入站消息监听（长轮询），将消息投递到 IMGateway
    pub fn start_polling(&self, incoming_tx: mpsc::UnboundedSender<IncomingMessage>) {
        let client = self.client.clone();
        let account_id = self.account_id.clone();
        let account_name = self.account_name.clone();
        tokio::spawn(async move {
            tracing::info!("[微信] 长轮询已启动: {}", account_id);
            let mut consecutive_failures = 0;

            loop {
                match client.get_updates().await {
                    Ok(resp) => {
                        consecutive_failures = 0;

                        if resp.errcode == Some(-14) {
                            tracing::warn!("[微信] 会话过期，暂停1小时");
                            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                            continue;
                        }

                        if let Some(msgs) = resp.msgs {
                            for msg in &msgs {
                                match convert_incoming_async(msg, &account_id, &account_name, &client.cdn_base_url).await {
                                    Some(incoming) => {
                                        let _ = incoming_tx.send(incoming);
                                    }
                                    None => {
                                        tracing::debug!("[微信] 消息被过滤: message_id={:?}", msg.message_id);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        tracing::warn!("[微信] getUpdates失败 (第{}次): {}", consecutive_failures, e);
                        if consecutive_failures >= 3 {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            consecutive_failures = 0;
                        }
                    }
                }
            }
        });
    }
}

/// 下载并解密微信 CDN 图片，转换为 Base64 data URL
async fn download_weixin_cdn_image_to_base64(
    encrypt_query_param: &str,
    aes_key_base64: &str,
    full_url: Option<&str>,
    cdn_base_url: &str,
) -> Result<String, AppError> {
    tracing::info!("[微信CDN] 开始下载图片: encrypt_param前缀={:?}", 
        &encrypt_query_param[..encrypt_query_param.len().min(40)]);

    // 1. 解析 AES key（base64 → 16 字节）
    let aes_key = base64::engine::general_purpose::STANDARD
        .decode(aes_key_base64)
        .map_err(|e| AppError::External(format!("AES key base64 解码失败: {}", e)))?;

    tracing::info!("[微信CDN] AES key 解码完成: {} 字节", aes_key.len());

    if aes_key.len() != 16 {
        // 兼容 hex 编码的 aes_key：base64 → 32 字符 hex → 16 字节
        if aes_key.len() == 32 {
            let hex_str = String::from_utf8_lossy(&aes_key);
            tracing::info!("[微信CDN] AES key 是 hex 编码, 尝试解析: {}", &hex_str);
            match simple_hex_decode(hex_str.trim()) {
                Some(decoded) if decoded.len() == 16 => {
                    return download_and_decrypt(encrypt_query_param, full_url, cdn_base_url, &decoded).await;
                }
                _ => {
                    return Err(AppError::External(format!(
                        "AES key hex 解码失败: 长度={}, 内容={}", aes_key.len(), hex_str
                    )));
                }
            }
        }
        return Err(AppError::External(format!(
            "AES key 解码后长度不是 16 字节: {} 字节", aes_key.len()
        )));
    }

    download_and_decrypt(encrypt_query_param, full_url, cdn_base_url, &aes_key).await
}

/// 下载 CDN 加密图片 → AES-128-ECB 解密 → Base64 data URL
async fn download_and_decrypt(
    encrypt_query_param: &str,
    full_url: Option<&str>,
    cdn_base_url: &str,
    aes_key: &[u8],
) -> Result<String, AppError> {
    // 2. 构建 CDN 下载 URL
    let url = match full_url.filter(|u| !u.is_empty()) {
        Some(full) => full.to_string(),
        None => crate::weixin::cdn::build_cdn_download_url(cdn_base_url, encrypt_query_param),
    };

    tracing::info!("[微信CDN] 下载 URL: {}", url);

    // 3. 下载加密的图片数据
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| AppError::External(format!("CDN 图片下载失败: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError::External(format!(
            "CDN 图片下载返回 HTTP {}",
            resp.status()
        )));
    }

    let encrypted = resp
        .bytes()
        .await
        .map_err(|e| AppError::External(format!("读取 CDN 响应失败: {}", e)))?;

    tracing::info!("[微信CDN] 下载完成: {} 字节（加密态）", encrypted.len());

    // 4. AES-128-ECB 解密
    let decrypted = crate::weixin::cdn::decrypt_aes_ecb(&encrypted, aes_key)?;

    tracing::info!("[微信CDN] 解密完成: {} 字节（明文）", decrypted.len());

    // 5. 探测图片格式
    let ext = detect_image_format(&decrypted);
    tracing::info!("[微信CDN] 图片格式: {}", ext);

    // 6. 转换为 Base64 data URL
    let b64 = base64::engine::general_purpose::STANDARD.encode(&decrypted);
    let mime = match ext {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    let data_url = format!("data:{};base64,{}", mime, b64);

    tracing::info!(
        "[微信CDN] 图片转 data URL 完成, 长度={}, 前缀={:?}",
        data_url.len(),
        &data_url[..data_url.len().min(60)]
    );

    Ok(data_url)
}

/// 简易 hex 解码（不使用 hex crate）
fn simple_hex_decode(hex_str: &str) -> Option<Vec<u8>> {
    let hex_str = hex_str.trim();
    if hex_str.len() % 2 != 0 {
        return None;
    }
    (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16).ok())
        .collect()
}

/// 下载微信 CDN 媒体文件并返回解密后的原始字节（通用函数）
async fn download_weixin_cdn_media_bytes(
    encrypt_query_param: &str,
    aes_key_base64: &str,
    full_url: Option<&str>,
    cdn_base_url: &str,
) -> Result<Vec<u8>, AppError> {
    // 1. 解析 AES key
    let aes_key = base64::engine::general_purpose::STANDARD
        .decode(aes_key_base64)
        .map_err(|e| AppError::External(format!("AES key base64 解码失败: {}", e)))?;

    if aes_key.len() != 16 {
        if aes_key.len() == 32 {
            let hex_str = String::from_utf8_lossy(&aes_key);
            match simple_hex_decode(hex_str.trim()) {
                Some(key) if key.len() == 16 => {
                    return download_raw_and_decrypt(encrypt_query_param, full_url, cdn_base_url, &key).await;
                }
                _ => {
                    return Err(AppError::External(format!(
                        "AES key hex 解码失败: 长度={}", aes_key.len()
                    )));
                }
            }
        }
        return Err(AppError::External(format!(
            "AES key 解码后长度不是 16 字节: {} 字节", aes_key.len()
        )));
    }

    download_raw_and_decrypt(encrypt_query_param, full_url, cdn_base_url, &aes_key).await
}

/// 执行 CDN 下载和 AES 解密，返回原始字节
async fn download_raw_and_decrypt(
    encrypt_query_param: &str,
    full_url: Option<&str>,
    cdn_base_url: &str,
    aes_key: &[u8],
) -> Result<Vec<u8>, AppError> {
    let url = match full_url.filter(|u| !u.is_empty()) {
        Some(full) => full.to_string(),
        None => crate::weixin::cdn::build_cdn_download_url(cdn_base_url, encrypt_query_param),
    };

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| AppError::External(format!("CDN 媒体下载失败: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError::External(format!("CDN 媒体下载返回 HTTP {}", resp.status())));
    }

    let encrypted = resp.bytes().await
        .map_err(|e| AppError::External(format!("读取 CDN 响应失败: {}", e)))?;

    let decrypted = crate::weixin::cdn::decrypt_aes_ecb(&encrypted, aes_key)?;
    Ok(decrypted)
}

/// 检测文件是否为文本类型（根据扩展名）
fn is_text_file(file_name: &str) -> bool {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(
        ext.as_str(),
        "txt" | "md" | "json" | "csv" | "log" | "xml"
            | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf"
            | "sh" | "bat" | "ps1" | "py" | "js" | "ts" | "rs"
            | "go" | "java" | "c" | "cpp" | "h" | "hpp"
            | "sql" | "html" | "css" | "scss" | "less"
            | "yaml" | "yml" | "json" | "xml"
    )
}

/// 通过文件头探测图片格式
fn detect_image_format(data: &[u8]) -> &'static str {
    if data.len() < 4 {
        return "jpg";
    }
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        "jpg"
    } else if data[0] == 0x89 && data[1] == b'P' && data[2] == b'N' && data[3] == b'G' {
        "png"
    } else if data[0] == b'G' && data[1] == b'I' && data[2] == b'F' {
        "gif"
    } else if data[0] == 0x52 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x46 {
        "webp"
    } else {
        "jpg"
    }
}

/// 将微信消息转为统一 IncomingMessage（异步版本，支持 CDN 图片下载）
async fn convert_incoming_async(
    msg: &crate::weixin::client::WeixinMessage,
    account_id: &str,
    account_name: &Option<String>,
    cdn_base_url: &str,
) -> Option<IncomingMessage> {
    // 只处理用户消息
    if msg.message_type != Some(1) {
        tracing::debug!("[微信] 跳过非用户消息: message_type={:?}", msg.message_type);
        return None;
    }

    let items = msg.item_list.as_deref().unwrap_or(&[]);

    // 日志：记录原始消息结构
    let item_types: Vec<i32> = items.iter().map(|i| i.item_type).collect();
    tracing::info!(
        "[微信] 收到用户消息: message_id={:?}, from={:?}, group_id={:?}, item_types={:?}",
        msg.message_id, msg.from_user_id, msg.group_id, item_types
    );

    // 提取文本内容（可能有多个 text_item，合并）
    let mut text: String = items
        .iter()
        .filter(|item| item.item_type == msg_item_type::TEXT)
        .filter_map(|item| item.text_item.as_ref())
        .filter_map(|t| Some(t.text.clone()))
        .collect::<Vec<_>>()
        .join(" ");

    tracing::info!("[微信] 提取文本: {:?}", if text.is_empty() { "(空)" } else { &text });

    // 处理媒体项
    let mut media_urls: Vec<String> = Vec::new();
    let mut has_non_image_media = false;
    let mut attachments: Vec<Attachment> = Vec::new();
    let mut video_data_urls: Vec<String> = Vec::new();

    for item in items {
        match item.item_type {
            msg_item_type::IMAGE => {
                tracing::info!("[微信] 检测到图片消息项");

                if let Some(img) = &item.image_item {
                    let has_media = img.media.is_some();
                    tracing::info!(
                        "[微信] 图片项详情: has_media={}, has_aeskey={}, mid_size={:?}",
                        has_media,
                        img.media.as_ref().and_then(|m| m.aes_key.as_ref()).is_some(),
                        img.mid_size
                    );

                    if let Some(media) = &img.media {
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");
                        let full_url = media.full_url.as_deref();

                        tracing::info!(
                            "[微信] CDN 信息: encrypt_param存在={}({}字节), aes_key存在={}({}字节), full_url={:?}",
                            !encrypt_param.is_empty(),
                            encrypt_param.len(),
                            !aes_key.is_empty(),
                            aes_key.len(),
                            full_url
                        );

                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_image_to_base64(
                                encrypt_param,
                                aes_key,
                                full_url,
                                cdn_base_url,
                            )
                            .await
                            {
                                Ok(data_url) => {
                                    tracing::info!(
                                        "[微信] CDN 图片下载+解密成功, data_url 长度={}",
                                        data_url.len()
                                    );
                                    media_urls.push(data_url);
                                }
                                Err(e) => {
                                    tracing::warn!("[微信] CDN 图片下载+解密失败: {}", e);
                                }
                            }
                        } else {
                            tracing::warn!(
                                "[微信] CDN 图片缺少必要参数: encrypt_param={}, aes_key={}",
                                encrypt_param, aes_key
                            );
                        }
                    } else {
                        tracing::warn!("[微信] 图片项无 media 字段");
                    }
                } else {
                    tracing::warn!("[微信] item_type=IMAGE 但 image_item 为 None");
                }
            }
            msg_item_type::FILE => {
                tracing::info!("[微信] 检测到文件消息项");
                if let Some(file) = &item.file_item {
                    let file_name = file.file_name.as_deref().unwrap_or("未知文件");
                    let file_size_str = file.len.as_deref().unwrap_or("0");
                    tracing::info!("[微信] 文件项: name={}, size={}", file_name, file_size_str);

                    if let Some(media) = &file.media {
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");

                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_media_bytes(
                                encrypt_param,
                                aes_key,
                                media.full_url.as_deref(),
                                cdn_base_url,
                            )
                            .await
                            {
                                Ok(bytes) => {
                                    has_non_image_media = true;
                                    // 保存为附件，由 IMGateway 统一持久化到磁盘
                                    attachments.push(Attachment {
                                        file_name: file_name.to_string(),
                                        data: bytes,
                                        mime_type: "application/octet-stream".to_string(),
                                    });
                                    let desc = format!("[用户发送了一个文件: {}]", file_name);
                                    text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                                }
                                Err(e) => {
                                    let desc = if text.is_empty() {
                                        format!("[用户发送了一个文件: {} (下载失败: {})]", file_name, e)
                                    } else {
                                        format!("{}\n[文件 {} 下载失败: {}]", text, file_name, e)
                                    };
                                    text = desc;
                                }
                            }
                        } else {
                            let desc = format!("[用户发送了一个文件: {}]", file_name);
                            text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                        }
                    } else {
                        let desc = format!("[用户发送了一个文件: {}]", file_name);
                        text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                    }
                }
            }
            msg_item_type::VIDEO => {
                tracing::info!("[微信] 检测到视频消息项");
                if let Some(video) = &item.video_item {
                    tracing::info!("[微信] 视频项: video_size={:?}", video.video_size);

                    if let Some(media) = &video.media {
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");

                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_media_bytes(
                                encrypt_param,
                                aes_key,
                                media.full_url.as_deref(),
                                cdn_base_url,
                            )
                            .await
                            {
                                Ok(bytes) => {
                                    has_non_image_media = true;
                                    let raw_len = bytes.len();
                                    // 小米全模态限制 base64 ≤ 50MB，原始文件需 ≤ ~37MB
                                    if raw_len <= 37 * 1024 * 1024 {
                                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                        let data_url = format!("data:video/mp4;base64,{}", b64);
                                        video_data_urls.push(data_url);
                                        tracing::info!("[微信] 视频已转换为 base64, 原始大小={}MB", raw_len / 1024 / 1024);
                                    } else {
                                        tracing::warn!("[微信] 视频过大 ({}MB > 37MB)，跳过 base64 编码", raw_len / 1024 / 1024);
                                    }
                                    // 保存为附件，由 IMGateway 统一持久化到磁盘
                                    let video_name = format!("video_{}.mp4", Uuid::new_v4());
                                    attachments.push(Attachment {
                                        file_name: video_name,
                                        data: bytes.to_vec(),
                                        mime_type: "video/mp4".to_string(),
                                    });
                                    let desc = if text.is_empty() {
                                        "[用户发送了一个视频]".to_string()
                                    } else {
                                        format!("{}\n[附带视频]", text)
                                    };
                                    text = desc;
                                }
                                Err(e) => {
                                    let desc = format!("[视频下载失败: {}]", e);
                                    text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                                }
                            }
                        } else {
                            let desc = "[用户发送了一个视频]".to_string();
                            text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                        }
                    } else {
                        let desc = "[用户发送了一个视频]".to_string();
                        text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                    }
                }
            }
            msg_item_type::VOICE => {
                tracing::info!("[微信] 检测到语音消息项");
                if let Some(voice) = &item.voice_item {
                    // 服务端有语音转文字结果，直接使用
                    if let Some(transcript) = &voice.text {
                        let transcript = transcript.trim();
                        if !transcript.is_empty() {
                            tracing::info!("[微信] 语音转文字: {}", transcript);
                            let desc = format!("[用户发送了一条语音, 转文字: {}]", transcript);
                            text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                            has_non_image_media = true;
                        }
                    } else if let Some(media) = &voice.media {
                        // 无转文字结果，下载语音文件
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");

                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_media_bytes(
                                encrypt_param,
                                aes_key,
                                media.full_url.as_deref(),
                                cdn_base_url,
                            )
                            .await
                            {
                                Ok(bytes) => {
                                    has_non_image_media = true;
                                    let size_kb = bytes.len() as f64 / 1024.0;
                                    let desc = if text.is_empty() {
                                        format!("[用户发送了一条语音 ({:.1} KB, SILK 格式)]", size_kb)
                                    } else {
                                        format!("{}\n[附带语音 ({:.1} KB, SILK 格式)]", text, size_kb)
                                    };
                                    text = desc;
                                }
                                Err(e) => {
                                    let desc = format!("[语音下载失败: {}]", e);
                                    text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                                }
                            }
                        } else {
                            let desc = "[用户发送了一条语音]".to_string();
                            text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                        }
                    } else {
                        let desc = "[用户发送了一条语音]".to_string();
                        text = if text.is_empty() { desc } else { format!("{}\n{}", text, desc) };
                    }
                }
            }
            _ => {}
        }
    }

    // 纯媒体消息（无文字）不能丢弃
    let has_any_media = !media_urls.is_empty() || has_non_image_media;
    let final_text = if text.is_empty() {
        if has_any_media {
            // 前面已设置描述文本，这里不会到达
            tracing::info!("[微信] 纯媒体消息，已生成描述");
            text
        } else {
            tracing::warn!("[微信] 消息无文字也无媒体，跳过: message_id={:?}", msg.message_id);
            return None;
        }
    } else {
        text
    };

    let from = msg.from_user_id.as_deref().unwrap_or("");
    let conv_id = msg.group_id.as_deref().unwrap_or(from);

    tracing::info!(
        "[微信] 转换完成: message_id={:?}, text={:?}, media_urls数量={}, from={:?}",
        msg.message_id,
        &final_text[..final_text.len().min(50)],
        media_urls.len(),
        from
    );

    Some(IncomingMessage {
        id: format!("wx_{}", msg.message_id.unwrap_or(0)),
        account_id: account_id.to_string(),
        account_name: account_name.clone(),
        platform: PlatformType::Custom("weixin".to_string()),
        conversation_id: conv_id.to_string(),
        sender_id: Some(from.to_string()),
        sender_staff_id: None,
        sender_name: None,
        text: final_text,
        media_urls,
        video_data_urls,
        attachments,
        raw: serde_json::json!(msg),
        session_webhook: None,
        // iLink SDK 不支持群聊，统一按私聊处理
        conversation_type: ConversationType::Private,
        conversation_title: None,
        timestamp: msg.create_time_ms.unwrap_or(0),
    })
}

/// 简易 Markdown 转纯文本
fn strip_markdown(md: &str) -> String {
    let mut result = md.to_string();
    result = result.replace("```", "");
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            result.replace_range(start..=start + end, "");
        } else {
            break;
        }
    }
    let re = regex::Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
    result = re.replace_all(&result, "$1").to_string();
    result = result.replace("# ", "");
    result = result.replace("## ", "");
    result = result.replace("### ", "");
    result = result.replace("**", "");
    result = result.replace("__", "");
    result
}

/// 下载远程图片到本地临时文件，返回临时文件路径
async fn download_remote_image(url: &str) -> Result<String, AppError> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| AppError::External(format!("下载远程图片失败: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError::External(format!(
            "下载远程图片返回非成功状态: {}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::External(format!("读取图片字节失败: {}", e)))?;

    // 写入临时文件
    let ext = url
        .split('?')
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("jpg");
    let temp_dir = std::env::temp_dir();
    let file_name = format!("weixin_img_{}.{}", Uuid::new_v4(), ext);
    let temp_path = temp_dir.join(file_name);

    tokio::fs::write(&temp_path, &bytes)
        .await
        .map_err(|e| AppError::External(format!("写入临时文件失败: {}", e)))?;

    Ok(temp_path.to_string_lossy().to_string())
}

#[async_trait]
impl IMAdapter for WeixinAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Custom("weixin".to_string())
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: false,
            supports_images: true,
            supports_files: true,
            max_message_length: 4000,
        }
    }

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError> {
        self.client.send_text(&target.conversation_id, text, None).await?;
        Ok(SendResult::ok())
    }

    async fn send_markdown(&self, _target: &MessageTarget, _title: &str, text: &str) -> Result<SendResult, AppError> {
        let plain = strip_markdown(text);
        self.send_text(_target, &plain).await
    }

    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError> {
        let sender_id = original.sender_id.as_deref().unwrap_or("");
        let ctx = original.raw.get("context_token").and_then(|v| v.as_str());
        self.client.send_text(sender_id, text, ctx).await?;
        Ok(SendResult::ok())
    }

    async fn send_image(
        &self,
        target: &MessageTarget,
        url: &str,
        caption: Option<&str>,
    ) -> Result<SendResult, AppError> {
        tracing::info!("[微信] 发送图片: target={}, url存在={}, caption={:?}",
            target.conversation_id, !url.is_empty(), caption);

        // 1. 下载远程图片到临时文件
        let temp_path = download_remote_image(url).await?;

        // 2. 上传到微信 CDN
        let uploaded = upload::upload_file_to_weixin(
            &self.client,
            &temp_path,
            &target.conversation_id,
        )
        .await?;

        tracing::info!("[微信] 图片 CDN 上传完成: filekey={}, fileSize={}",
            uploaded.filekey, uploaded.file_size);

        // 3. 发送图片消息（附带 caption 作为文本前缀）
        let caption_text = caption.unwrap_or("");
        self.client
            .send_image_message(
                &target.conversation_id,
                caption_text,
                &uploaded.download_param,
                &uploaded.aeskey_base64,
                uploaded.file_size_ciphertext,
                None,
            )
            .await?;

        // 4. 清理临时文件
        let _ = tokio::fs::remove_file(&temp_path).await;

        tracing::info!("[微信] 图片发送成功");
        Ok(SendResult::ok())
    }

    async fn send_file(
        &self,
        target: &MessageTarget,
        url: &str,
        file_name: &str,
    ) -> Result<SendResult, AppError> {
        tracing::info!("[微信] 发送文件: target={}, url存在={}, fileName={}",
            target.conversation_id, !url.is_empty(), file_name);

        // 1. 下载远程文件到临时文件
        let temp_path = download_remote_image(url).await?;

        // 2. 上传到微信 CDN
        let uploaded = upload::upload_file_attachment_to_weixin(
            &self.client,
            &temp_path,
            &target.conversation_id,
        )
        .await?;

        // 3. 发送文件消息
        self.client
            .send_file_message(
                &target.conversation_id,
                "",
                file_name,
                &uploaded.download_param,
                &uploaded.aeskey_base64,
                uploaded.file_size,
                None,
            )
            .await?;

        // 4. 清理临时文件
        let _ = tokio::fs::remove_file(&temp_path).await;

        tracing::info!("[微信] 文件发送成功");
        Ok(SendResult::ok())
    }

    async fn send_video(
        &self,
        target: &MessageTarget,
        url: &str,
        caption: Option<&str>,
    ) -> Result<SendResult, AppError> {
        tracing::info!("[微信] 发送视频: target={}, url存在={}, caption={:?}",
            target.conversation_id, !url.is_empty(), caption);

        // 1. 下载远程视频到临时文件
        let temp_path = download_remote_image(url).await?;

        // 2. 上传到微信 CDN（使用 VIDEO 媒体类型）
        let uploaded = upload::upload_video_to_weixin(
            &self.client,
            &temp_path,
            &target.conversation_id,
        )
        .await?;

        tracing::info!("[微信] 视频 CDN 上传完成: filekey={}, fileSize={}",
            uploaded.filekey, uploaded.file_size);

        // 3. 发送视频消息
        let caption_text = caption.unwrap_or("");
        self.client
            .send_video_message(
                &target.conversation_id,
                caption_text,
                &uploaded.download_param,
                &uploaded.aeskey_base64,
                uploaded.file_size_ciphertext,
                None,
            )
            .await?;

        // 4. 清理临时文件
        let _ = tokio::fs::remove_file(&temp_path).await;

        tracing::info!("[微信] 视频发送成功");
        Ok(SendResult::ok())
    }

    async fn start_stream_reply(&self, _original: &IncomingMessage) -> Result<mpsc::UnboundedSender<String>, AppError> {
        Err(AppError::External("微信不支持流式回复".to_string()))
    }

    async fn finish_stream_reply(&self, _original: &IncomingMessage) -> Result<(), AppError> {
        Ok(())
    }
}