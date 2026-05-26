use axum::{extract::Query, Json, Router};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

static LOGIN_SESSIONS: once_cell::sync::Lazy<Arc<Mutex<HashMap<String, LoginSession>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

#[derive(Debug, Clone)]
struct LoginSession {
    qrcode: String,
    status: String,
    bot_token: Option<String>,
    ilink_bot_id: Option<String>,
    base_url: Option<String>,
}

#[derive(Deserialize)]
struct QrQuery { bot_type: Option<String> }

#[derive(Serialize)]
struct QrResp { success: bool, data: Option<QrData>, message: Option<String> }

#[derive(Serialize)]
struct QrData { session: String, qrcode_url: String }

#[derive(Deserialize)]
struct StQuery { session: String }

#[derive(Serialize)]
struct StResp {
    success: bool, status: String,
    bot_token: Option<String>, ilink_bot_id: Option<String>, base_url: Option<String>,
    message: Option<String>,
}

/// 获取微信扫码二维码
async fn get_qrcode(Query(_q): Query<QrQuery>) -> Json<QrResp> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    // 请求体携带 local_token_list 辅助服务器判断
    let request_body = serde_json::json!({
        "local_token_list": []
    });

    match client
        .post("https://ilinkai.weixin.qq.com/ilink/bot/get_bot_qrcode?bot_type=3")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send().await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => {
                let qrcode = body["qrcode"].as_str().unwrap_or("").to_string();
                // 使用 API 返回的完整确认 URL（包含 pass_ticket 等参数）
                let qr_content = body["qrcode_img_content"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();

                let session_id = uuid::Uuid::new_v4().to_string();
                LOGIN_SESSIONS.lock().await.insert(session_id.clone(), LoginSession {
                    qrcode: qrcode.clone(), status: "wait".to_string(),
                    bot_token: None, ilink_bot_id: None, base_url: None,
                });
                Json(QrResp {
                    success: true,
                    data: Some(QrData {
                        session: session_id,
                        qrcode_url: qr_content,
                    }),
                    message: None,
                })
            }
            Err(e) => Json(QrResp { success: false, data: None, message: Some(format!("解析失败: {}", e)) }),
        },
        Err(e) => Json(QrResp { success: false, data: None, message: Some(format!("请求失败: {}", e)) }),
    }
}

/// 轮询扫码状态（35秒长轮询）
async fn get_status(Query(q): Query<StQuery>) -> Json<StResp> {
    let qrcode = {
        let sessions = LOGIN_SESSIONS.lock().await;
        sessions.get(&q.session).map(|s| s.qrcode.clone())
    };
    let qrcode = match qrcode {
        Some(s) => s,
        None => return Json(StResp {
            success: false, status: "invalid".to_string(),
            bot_token: None, ilink_bot_id: None, base_url: None,
            message: Some("会话不存在".to_string()),
        }),
    };

    let client = reqwest::Client::new();
    match client
        .get(format!("https://ilinkai.weixin.qq.com/ilink/bot/get_qrcode_status?qrcode={}", qrcode))
        .timeout(std::time::Duration::from_secs(35))
        .send().await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => {
                let status = body["status"].as_str().unwrap_or("wait").to_string();
                if status == "confirmed" {
                    let token = body["bot_token"].as_str().unwrap_or("").to_string();
                    let bot_id = body["ilink_bot_id"].as_str().unwrap_or("").to_string();
                    let base_url = body["baseurl"].as_str().unwrap_or("https://ilinkai.weixin.qq.com").to_string();
                    let mut sessions = LOGIN_SESSIONS.lock().await;
                    if let Some(s) = sessions.get_mut(&q.session) {
                        s.status = "confirmed".to_string();
                        s.bot_token = Some(token.clone());
                        s.ilink_bot_id = Some(bot_id.clone());
                        s.base_url = Some(base_url.clone());
                    }
                    Json(StResp {
                        success: true, status: "confirmed".to_string(),
                        bot_token: Some(token), ilink_bot_id: Some(bot_id), base_url: Some(base_url),
                        message: None,
                    })
                } else {
                    let mut sessions = LOGIN_SESSIONS.lock().await;
                    if let Some(s) = sessions.get_mut(&q.session) { s.status = status.clone(); }
                    Json(StResp {
                        success: true, status,
                        bot_token: None, ilink_bot_id: None, base_url: None, message: None,
                    })
                }
            }
            Err(e) => {
                // 长轮询超时或解析失败是正常现象，返回 wait 让前端继续轮询
                tracing::debug!("[微信] 解析状态响应失败: {}", e);
                Json(StResp {
                    success: true, status: "wait".to_string(),
                    bot_token: None, ilink_bot_id: None, base_url: None, message: None,
                })
            }
        },
        Err(e) => {
            // 网络超时是长轮询的正常行为，返回 wait 让前端继续轮询
            tracing::debug!("[微信] 状态轮询请求失败: {}", e);
            Json(StResp {
                success: true, status: "wait".to_string(),
                bot_token: None, ilink_bot_id: None, base_url: None, message: None,
            })
        },
    }
}

pub fn routes() -> Router {
    Router::new()
        .route("/weixin/qrcode", get(get_qrcode))
        .route("/weixin/status", get(get_status))
}
