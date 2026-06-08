//! 微信媒体文件上传管线
//!
//! 封装了完整的 CDN 上传流程：
//! 1. 读取本地文件 → 2. 计算 MD5 和大小 → 3. 生成 AES 密钥和文件标识
//! 4. 调用 getUploadUrl 获取上传 URL → 5. AES-128-ECB 加密文件
//! 6. 上传到 CDN → 7. 返回下载参数

use crate::error::AppError;
use crate::weixin::cdn;
use crate::weixin::client::{GetUploadUrlReq, WeixinClient};
use base64::Engine;
use md5::{Digest, Md5};
use std::path::Path;

/// CDN 上传完成后的文件信息
pub struct UploadedFileInfo {
    /// 文件标识
    pub filekey: String,
    /// 下载加密查询参数（用于 MessageItem.media.encrypt_query_param）
    pub download_param: String,
    /// AES-128 密钥（hex 编码）
    pub aeskey: String,
    /// AES-128 密钥（base64 编码，用于 CDNMedia.aes_key）
    pub aeskey_base64: String,
    /// 明文大小（字节）
    pub file_size: i64,
    /// 密文大小（PKCS7 padding 后）
    pub file_size_ciphertext: i64,
}

/// 生成 16 字节随机十六进制字符串
fn random_hex_16() -> String {
    // 用 UUID v4 生成随机 hex
    uuid::Uuid::new_v4().to_string().replace('-', "")[..32].to_string()
}

/// 通用媒体上传管线
///
/// 读取文件 → 哈希 → 生成密钥 → getUploadUrl → 加密 → 上传 CDN → 返回文件信息
async fn upload_media_to_cdn(
    client: &WeixinClient,
    file_path: &str,
    to_user_id: &str,
    media_type: i32,
) -> Result<UploadedFileInfo, AppError> {
    // 1. 读取文件
    let path = Path::new(file_path);
    let plaintext = tokio::fs::read(path)
        .await
        .map_err(|e| AppError::External(format!("读取文件失败 {}: {}", file_path, e)))?;

    let rawsize = plaintext.len() as i64;

    // 2. 计算 MD5
    let mut hasher = Md5::new();
    hasher.update(&plaintext);
    let rawfilemd5 = format!("{:x}", hasher.finalize());

    // 3. 生成密钥和文件标识
    let filesize = cdn::aes_ecb_padded_size(plaintext.len()) as i64;
    let filekey = random_hex_16();
    let aeskey_bytes = random_hex_16()[..16].as_bytes().to_vec();
    let aeskey_hex = hex_encode(&aeskey_bytes);

    // 4. 获取上传 URL
    let upload_req = GetUploadUrlReq {
        filekey: filekey.clone(),
        media_type,
        to_user_id: to_user_id.to_string(),
        rawsize,
        rawfilemd5,
        filesize,
        no_need_thumb: Some(true),
        aeskey: aeskey_hex.clone(),
        base_info: None,
    };

    let upload_url_resp = client.get_upload_url(&upload_req).await?;

    let upload_full_url = upload_url_resp.upload_full_url.as_deref();
    let upload_param = upload_url_resp.upload_param.as_deref();

    // 5. AES-128-ECB 加密
    let ciphertext = cdn::encrypt_aes_ecb(&plaintext, &aeskey_bytes)?;

    // 6. 上传到 CDN
    let cdn_result = cdn::upload_buffer_to_cdn(
        &ciphertext,
        upload_full_url,
        upload_param,
        &filekey,
        &client.cdn_base_url,
    )
    .await?;

    // 构建 base64 编码的 aes_key（用于 CDNMedia.aes_key）
    let aeskey_b64 = base64::engine::general_purpose::STANDARD.encode(&aeskey_bytes);

    Ok(UploadedFileInfo {
        filekey,
        download_param: cdn_result.download_param,
        aeskey: aeskey_hex,
        aeskey_base64: aeskey_b64,
        file_size: rawsize,
        file_size_ciphertext: filesize,
    })
}

/// 将字节切片编码为十六进制字符串
fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 上传图片文件到微信 CDN
pub async fn upload_file_to_weixin(
    client: &WeixinClient,
    file_path: &str,
    to_user_id: &str,
) -> Result<UploadedFileInfo, AppError> {
    upload_media_to_cdn(
        client,
        file_path,
        to_user_id,
        crate::weixin::client::upload_media_type::IMAGE,
    )
    .await
}

/// 上传视频文件到微信 CDN
pub async fn upload_video_to_weixin(
    client: &WeixinClient,
    file_path: &str,
    to_user_id: &str,
) -> Result<UploadedFileInfo, AppError> {
    upload_media_to_cdn(
        client,
        file_path,
        to_user_id,
        crate::weixin::client::upload_media_type::VIDEO,
    )
    .await
}

/// 上传普通文件附件到微信 CDN（非图片/视频）
pub async fn upload_file_attachment_to_weixin(
    client: &WeixinClient,
    file_path: &str,
    to_user_id: &str,
) -> Result<UploadedFileInfo, AppError> {
    upload_media_to_cdn(
        client,
        file_path,
        to_user_id,
        crate::weixin::client::upload_media_type::FILE,
    )
    .await
}