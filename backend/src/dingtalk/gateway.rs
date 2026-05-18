//! 钉钉流式网关连接管理
//!
//! 负责与钉钉网关通信，获取 WebSocket 端点地址和连接票据。

use crate::dingtalk::frames::{
    ConnectionResponse, GatewayRequest, Subscription, SUBSCRIPTION_CALLBACK, SUBSCRIPTION_EVENT,
    TOPIC_CARD, TOPIC_ROBOT, TOPIC_ROBOT_DELEGATE,
};
use crate::error::AppError;

/// 网关连接器
///
/// 负责向钉钉网关发起连接请求，换取 WebSocket 的 endpoint 和 ticket。
pub struct GatewayConnector;

impl GatewayConnector {
    /// 打开网关连接
    ///
    /// * `http_client` - 复用的 reqwest HTTP 客户端
    /// * `client_id` - 钉钉应用的 Client ID
    /// * `client_secret` - 钉钉应用的 Client Secret
    /// * `local_ip` - 本地 IP 地址（用于钉钉侧定位）
    /// * `subscribe_robot` - 是否订阅机器人消息
    /// * `subscribe_card` - 是否订阅卡片回调
    /// * `subscribe_delegate` - 是否订阅委派消息
    pub async fn open(
        http_client: &reqwest::Client,
        client_id: &str,
        client_secret: &str,
        local_ip: &str,
        subscribe_robot: bool,
        subscribe_card: bool,
        subscribe_delegate: bool,
    ) -> Result<ConnectionResponse, AppError> {
        let url = "https://api.dingtalk.com/v1.0/gateway/connections/open";

        let mut subscriptions = Vec::new();

        if subscribe_robot {
            subscriptions.push(Subscription {
                topic: TOPIC_ROBOT.to_string(),
                sub_type: SUBSCRIPTION_CALLBACK.to_string(),
            });
        }

        if subscribe_delegate {
            subscriptions.push(Subscription {
                topic: TOPIC_ROBOT_DELEGATE.to_string(),
                sub_type: SUBSCRIPTION_CALLBACK.to_string(),
            });
        }

        if subscribe_card {
            subscriptions.push(Subscription {
                topic: TOPIC_CARD.to_string(),
                sub_type: SUBSCRIPTION_EVENT.to_string(),
            });
        }

        let request = GatewayRequest {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            subscriptions,
            ua: format!("novaclaw-backend/{}", env!("CARGO_PKG_VERSION")),
            local_ip: local_ip.to_string(),
        };

        let resp = http_client
            .post(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::External(format!("钉钉网关连接请求失败: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::External(format!(
                "钉钉网关连接失败 (HTTP {}): {}",
                status, text
            )));
        }

        let conn_resp: ConnectionResponse = resp
            .json()
            .await
            .map_err(|e| AppError::External(format!("钉钉解析网关连接响应失败: {}", e)))?;

        Ok(conn_resp)
    }
}
