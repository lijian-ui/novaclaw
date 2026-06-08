//! 微信 CDN 上传/下载工具
//!
//! 提供 AES-128-ECB 加密解密、CDN URL 构建、文件上传到 CDN 等功能。
//! 参考腾讯 `openclaw-weixin` 插件 CDN 实现。

use crate::error::AppError;

/// AES-128-ECB 加密块大小（字节）
const BLOCK_SIZE: usize = 16;

/// 计算 AES-128-ECB 密文大小（PKCS7 padding 后对齐到 16 字节边界）
pub fn aes_ecb_padded_size(plaintext_size: usize) -> usize {
    ((plaintext_size + BLOCK_SIZE - 1) / BLOCK_SIZE) * BLOCK_SIZE
}

/// AES-128-ECB 加密（带 PKCS7 padding）
///
/// - `plaintext`: 明文数据
/// - `key`: 16 字节 AES 密钥
/// - 返回加密后的密文
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, AppError> {
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::cipher::generic_array::GenericArray;

    if key.len() != 16 {
        return Err(AppError::Internal("AES 密钥长度必须为 16 字节".to_string()));
    }

    let cipher = aes::Aes128::new_from_slice(key)
        .map_err(|e| AppError::Internal(format!("AES 密钥初始化失败: {}", e)))?;

    // PKCS7 padding
    let pad_len = BLOCK_SIZE - (plaintext.len() % BLOCK_SIZE);
    let padded_len = plaintext.len() + pad_len;
    let mut padded = vec![0u8; padded_len];
    padded[..plaintext.len()].copy_from_slice(plaintext);
    // 填充字节值为填充长度
    for i in plaintext.len()..padded_len {
        padded[i] = pad_len as u8;
    }

    // ECB 模式：逐块独立加密
    for chunk in padded.chunks_mut(BLOCK_SIZE) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }

    Ok(padded)
}

/// AES-128-ECB 解密（移除 PKCS7 padding）
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>, AppError> {
    use aes::cipher::{BlockDecrypt, KeyInit};
    use aes::cipher::generic_array::GenericArray;

    if key.len() != 16 {
        return Err(AppError::Internal("AES 密钥长度必须为 16 字节".to_string()));
    }
    if ciphertext.len() % BLOCK_SIZE != 0 {
        return Err(AppError::Internal("密文长度必须是 16 的倍数".to_string()));
    }

    let cipher = aes::Aes128::new_from_slice(key)
        .map_err(|e| AppError::Internal(format!("AES 密钥初始化失败: {}", e)))?;

    let mut buf = ciphertext.to_vec();
    for chunk in buf.chunks_mut(BLOCK_SIZE) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }

    // 移除 PKCS7 padding
    let pad_len = buf.last().copied().unwrap_or(0) as usize;
    if pad_len == 0 || pad_len > BLOCK_SIZE {
        return Err(AppError::Internal("PKCS7 padding 校验失败".to_string()));
    }
    let result_len = buf.len() - pad_len;
    buf.truncate(result_len);

    Ok(buf)
}

/// 构建 CDN 上传 URL
///
/// 当服务端返回 `upload_full_url` 时优先使用该 URL；
/// 否则使用 `cdn_base_url` + `upload_param` + `filekey` 拼接。
pub fn build_cdn_upload_url(
    cdn_base_url: &str,
    upload_param: &str,
    filekey: &str,
) -> String {
    let cdn_base = cdn_base_url.trim_end_matches('/');
    format!(
        "{}/upload?encrypted_query_param={}&filekey={}",
        cdn_base,
        urlencoding::encode(upload_param),
        urlencoding::encode(filekey),
    )
}

/// 构建 CDN 下载 URL
pub fn build_cdn_download_url(
    cdn_base_url: &str,
    encrypted_query_param: &str,
) -> String {
    let cdn_base = cdn_base_url.trim_end_matches('/');
    format!(
        "{}/download?encrypted_query_param={}",
        cdn_base,
        urlencoding::encode(encrypted_query_param),
    )
}

/// CDN 上传结果
pub struct CdnUploadResult {
    /// 下载用的加密查询参数（用于 CDNMedia.encrypt_query_param）
    pub download_param: String,
}

/// 将加密后的缓冲区上传到微信 CDN
///
/// 使用 `upload_full_url`（优先）或 `cdn_base_url` + `upload_param` 拼接 URL，
/// 以 `application/octet-stream` 格式 POST 上传。
///
/// 最多重试 3 次，4xx 客户端错误直接终止。
pub async fn upload_buffer_to_cdn(
    ciphertext: &[u8],
    upload_full_url: Option<&str>,
    upload_param: Option<&str>,
    filekey: &str,
    cdn_base_url: &str,
) -> Result<CdnUploadResult, AppError> {
    let cdn_url = match upload_full_url.filter(|u| !u.trim().is_empty()) {
        Some(url) => url.trim().to_string(),
        None => {
            let param = upload_param
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| AppError::External("CDN 上传缺少 URL（需要 upload_full_url 或 upload_param）".to_string()))?;
            build_cdn_upload_url(cdn_base_url, param, filekey)
        }
    };

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Internal(format!("创建 HTTP 客户端失败: {}", e)))?;

    let max_retries = 3;
    let mut last_error: Option<AppError> = None;

    for attempt in 1..=max_retries {
        match http_client
            .post(&cdn_url)
            .header("Content-Type", "application/octet-stream")
            .body(ciphertext.to_vec())
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.as_u16() >= 400 && status.as_u16() < 500 {
                    let err_msg = resp
                        .headers()
                        .get("x-error-message")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    return Err(AppError::External(format!(
                        "CDN 上传客户端错误 (HTTP {}): {}",
                        status, err_msg
                    )));
                }
                if !status.is_success() {
                    let err_msg = resp
                        .headers()
                        .get("x-error-message")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or(&format!("HTTP {}", status))
                        .to_string();
                    last_error = Some(AppError::External(format!(
                        "CDN 上传服务器错误 (尝试 {}/{}): {}",
                        attempt, max_retries, err_msg
                    )));
                    continue;
                }

                let download_param = resp
                    .headers()
                    .get("x-encrypted-param")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        AppError::External("CDN 上传响应缺少 x-encrypted-param 头".to_string())
                    })?;

                return Ok(CdnUploadResult {
                    download_param: download_param.to_string(),
                });
            }
            Err(e) => {
                last_error = Some(AppError::External(format!(
                    "CDN 上传请求失败 (尝试 {}/{}): {}",
                    attempt, max_retries, e
                )));
                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::External("CDN 上传失败：未知错误".to_string())))
}