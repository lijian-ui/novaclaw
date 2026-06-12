//! 钉钉 AI Card 流式消息实现
//!
//! 基于 OpenClaw dingtalk-openclaw-connector 的实现方案。

use crate::dingtalk::credential::TokenManager;
use crate::error::AppError;
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const AI_CARD_TEMPLATE_ID: &str = "02fcf2f4-5e02-4a85-b672-46d1f715543e.schema";
const API_BASE: &str = "https://api.dingtalk.com";

/// AI Card 实例
#[derive(Clone)]
pub struct AICardInstance {
    pub card_instance_id: String,
    pub access_token: String,
}

/// 卡片消息管理器
pub struct CardSender {
    http_client: reqwest::Client,
    token_manager: Arc<TokenManager>,
    robot_code: String,
}

impl CardSender {
    pub fn new(http_client: reqwest::Client, token_manager: Arc<TokenManager>, robot_code: String) -> Self {
        Self { http_client, token_manager, robot_code }
    }

    /// 创建并投放 AI Card
    pub async fn create(
        &self,
        target_user_id: Option<&str>,
        target_open_conversation_id: Option<&str>,
    ) -> Result<AICardInstance, AppError> {
        let token = self.token_manager.get_token().await?;
        let card_instance_id = format!("card_{}_{}", 
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            &uuid::Uuid::new_v4().simple().to_string()[..8]);

        // 创建卡片实例
        let create_body = json!({
            "cardTemplateId": AI_CARD_TEMPLATE_ID,
            "outTrackId": card_instance_id,
            "cardData": {
                "cardParamMap": {
                    "config": r#"{"autoLayout":true}"#,
                }
            },
            "callbackType": "STREAM",
            "imGroupOpenSpaceModel": { "supportForward": true },
            "imRobotOpenSpaceModel": { "supportForward": true },
        });

        let resp = self.http_client
            .post(format!("{}/v1.0/card/instances", API_BASE))
            .header("x-acs-dingtalk-access-token", &token)
            .header("Content-Type", "application/json")
            .json(&create_body)
            .send()
            .await
            .map_err(|e| AppError::External(format!("创建 AI Card 实例失败: {}", e)))?;

        let status = resp.status();
        let create_text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::External(format!("创建 AI Card 实例失败 (HTTP {}): {}", status, create_text)));
        }
        tracing::info!("创建响应: {}", &create_text[..create_text.len().min(200)]);

        // 投放卡片
        let deliver_body = if let Some(user_id) = target_user_id {
            json!({
                "outTrackId": card_instance_id,
                "openSpaceId": format!("dtv1.card//IM_ROBOT.{}", user_id),
                "userIdType": 1,
                "imRobotOpenDeliverModel": {
                    "robotCode": self.robot_code,
                    "spaceType": "IM_ROBOT",
                    "extension": { "dynamicSummary": "true" },
                },
            })
        } else if let Some(conv_id) = target_open_conversation_id {
            json!({
                "outTrackId": card_instance_id,
                "openSpaceId": format!("dtv1.card//IM_GROUP.{}", conv_id),
                "userIdType": 1,
                "imGroupOpenDeliverModel": {
                    "robotCode": self.robot_code,
                },
            })
        } else {
            return Err(AppError::External("缺少目标用户或群聊 ID".to_string()));
        };

        let del_resp = self.http_client
            .post(format!("{}/v1.0/card/instances/deliver", API_BASE))
            .header("x-acs-dingtalk-access-token", &token)
            .header("Content-Type", "application/json")
            .json(&deliver_body)
            .send()
            .await
            .map_err(|e| AppError::External(format!("投放 AI Card 失败: {}", e)))?;

        let del_status = del_resp.status();
        let del_text = del_resp.text().await.unwrap_or_default();
        if !del_status.is_success() {
            return Err(AppError::External(format!("投放 AI Card 失败 (HTTP {}): {}", del_status, del_text)));
        }
        tracing::info!("投放响应: {}", &del_text[..del_text.len().min(200)]);

        tracing::info!("AI Card 创建并投放成功: cardInstanceId={}", card_instance_id);
        Ok(AICardInstance { card_instance_id, access_token: token })
    }

    /// 设置卡片状态为 INPUTING（首次内容更新时调用一次）
    pub async fn set_inputing(&self, card: &AICardInstance, content: &str) -> Result<(), AppError> {
        let status_body = json!({
            "outTrackId": card.card_instance_id,
            "cardData": {
                "cardParamMap": {
                    "flowStatus": "2",
                    "msgContent": content,
                    "staticMsgContent": "",
                    "sys_full_json_obj": r#"{"order":["msgContent"]}"#,
                    "config": r#"{"autoLayout":true}"#,
                }
            },
        });

        let resp = self.http_client
            .put(format!("{}/v1.0/card/instances", API_BASE))
            .header("x-acs-dingtalk-access-token", &card.access_token)
            .header("Content-Type", "application/json")
            .json(&status_body)
            .send()
            .await
            .map_err(|e| AppError::External(format!("AI Card set INPUTING 失败: {}", e)))?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::External(format!("AI Card set INPUTING 失败 (HTTP {}): {}", status, text)));
        }
        tracing::debug!("[AI Card] INPUTING 响应: {}", &text[..text.len().min(200)]);
        Ok(())
    }

    /// 流式更新卡片内容
    pub async fn stream_update(&self, card: &AICardInstance, content: &str, is_finalize: bool) -> Result<(), AppError> {
        let guid = format!("{:x}_{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            &uuid::Uuid::new_v4().simple().to_string()[..8]);

        let body = json!({
            "outTrackId": card.card_instance_id,
            "guid": guid,
            "key": "msgContent",
            "content": content,
            "isFull": true,
            "isFinalize": is_finalize,
            "isError": false,
        });

        let resp = self.http_client
            .put(format!("{}/v1.0/card/streaming", API_BASE))
            .header("x-acs-dingtalk-access-token", &card.access_token)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::External(format!("AI Card 流式更新请求失败: {}", e)))?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::External(format!("AI Card 流式更新失败 (HTTP {}): {}", status, text)));
        }
        tracing::debug!("[AI Card] streaming 响应 (isFinalize={}): {}", is_finalize, &text[..text.len().min(200)]);
        Ok(())
    }

    /// 完成卡片（设置 FINISHED 状态）
    pub async fn set_finished(&self, card: &AICardInstance, content: &str) -> Result<(), AppError> {
        let status_body = json!({
            "outTrackId": card.card_instance_id,
            "cardData": {
                "cardParamMap": {
                    "flowStatus": "3",
                    "msgContent": content,
                    "staticMsgContent": "",
                    "sys_full_json_obj": r#"{"order":["msgContent"]}"#,
                    "config": r#"{"autoLayout":true}"#,
                }
            },
            "cardUpdateOptions": { "updateCardDataByKey": true },
        });

        let resp = self.http_client
            .put(format!("{}/v1.0/card/instances", API_BASE))
            .header("x-acs-dingtalk-access-token", &card.access_token)
            .header("Content-Type", "application/json")
            .json(&status_body)
            .send()
            .await
            .map_err(|e| AppError::External(format!("AI Card FINISHED 请求失败: {}", e)))?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::External(format!("AI Card FINISHED 失败 (HTTP {}): {}", status, text)));
        }
        tracing::info!("[AI Card] FINISHED 响应: {}", &text[..text.len().min(200)]);
        Ok(())
    }
}
