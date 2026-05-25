//! IM 配置热加载

use crate::dingtalk;
use crate::im::registry::AccountInfo;
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
                let account_ids = channel.enabled_account_ids();

                if account_ids.is_empty() {
                    if channel.use_webhook_mode() {
                        tracing::info!("钉钉 Webhook 模式已配置 (id={})", channel.id);
                    } else {
                        tracing::warn!("钉钉渠道 '{}' 没有有效的账号配置", channel.name);
                    }
                    continue;
                }

                for account_id in &account_ids {
                    let account_cfg = match channel.get_account(account_id) {
                        Some(c) => c,
                        None => { tracing::warn!("账号 '{}' 配置获取失败，跳过", account_id); continue; }
                    };

                    if !account_cfg.enabled {
                        tracing::info!("钉钉账号已禁用，跳过: {}", account_id);
                        continue;
                    }

                    tracing::info!("正在连接钉钉账号: {} (name={:?})", account_id, account_cfg.name);

                    let dt_client = std::sync::Arc::new(
                        dingtalk::DingTalkClient::new(
                            account_id.clone(),
                            account_cfg.name.clone(),
                            account_cfg.credentials.client_id.clone(),
                            account_cfg.credentials.client_secret.clone(),
                        )
                        .await,
                    );

                    let dt_adapter = std::sync::Arc::new(
                        dingtalk::adapter::DingTalkAdapter::new(dt_client.clone())
                    );

                    // 注册回调处理器
                    {
                        let incoming_tx = gateway.incoming_tx.clone();
                        let acc_id = account_id.clone();
                        dt_client
                            .register_handler(
                                crate::im::handler::IMGatewayCallbackHandler::new(
                                    incoming_tx,
                                    acc_id,
                                ),
                            )
                            .await;
                    }

                    gateway.register(AccountInfo {
                        account_id: account_id.clone(),
                        platform: crate::im::types::PlatformType::DingTalk,
                        adapter: dt_adapter.clone(),
                        enabled: true,
                        name: account_cfg.name.clone(),
                    }).await;

                    tracing::info!("钉钉账号已注册: {} (name={:?})", account_id, account_cfg.name);
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

    tracing::info!("IM 热加载完成");
}