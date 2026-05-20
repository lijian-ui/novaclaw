//! IM 配置热加载

use crate::dingtalk;
use crate::im::IMGateway;

/// 根据当前配置文件重新初始化所有 IM 连接
/// 会替换全局 IM_GATEWAY，旧连接的 WebSocket 自动关闭
pub async fn reload_gateway() {
    let im_config = crate::im::config::load();
    tracing::info!("IM 热加载: 共 {} 个渠道配置", im_config.channels.len());

    let gateway = IMGateway::new();

    for channel in &im_config.channels {
        if !channel.enabled {
            tracing::info!("IM 渠道已禁用，跳过: {}", channel.id);
            continue;
        }

        match channel.effective_type() {
            "dingtalk" => {
                if channel.use_stream_mode() {
                    tracing::info!("正在连接钉钉 Stream 模式...");
                    let cid = match channel.config.client_id.as_ref() {
                        Some(c) => c,
                        None => { tracing::warn!("钉钉渠道 '{}' 缺少 client_id，跳过", channel.name); continue; }
                    };
                    let cs = match channel.config.client_secret.as_ref() {
                        Some(c) => c,
                        None => { tracing::warn!("钉钉渠道 '{}' 缺少 client_secret，跳过", channel.name); continue; }
                    };
                    let dt_client = std::sync::Arc::new(dingtalk::DingTalkClient::new(cid.clone(), cs.clone()).await);

                    let dt_adapter = std::sync::Arc::new(dingtalk::adapter::DingTalkAdapter::new(dt_client.clone()));

                    // 注册回调处理器
                    {
                        let incoming_tx = gateway.incoming_tx.clone();
                        dt_client
                            .register_handler(
                                crate::im::handler::IMGatewayCallbackHandler::new(incoming_tx),
                            )
                            .await;
                    }

                    gateway.register(dt_adapter).await;
                    tracing::info!("钉钉 Stream 模式已注册到 IMGateway");
                } else if channel.use_webhook_mode() {
                    tracing::info!("钉钉 Webhook 模式已配置 (webhook={})",
                        channel.config.webhook.as_ref().map(|s| s.chars().take(40).collect::<String>()).unwrap_or_else(|| "?".to_string()));
                } else {
                    tracing::warn!("钉钉渠道 '{}' 没有有效的配置", channel.name);
                }
            }
            _ => {
                tracing::warn!("不支持的 IM 渠道类型: {} (id={})", channel.effective_type(), channel.id);
            }
        }
    }

    // 替换全局实例
    let mut g = crate::IM_GATEWAY.write().await;
    *g = Some(gateway);

    tracing::info!("IM 热加载完成 ({} 个渠道配置)", im_config.channels.len());
}
