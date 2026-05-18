//! 钉钉消息处理器特质和注册表（线程安全）

use crate::dingtalk::frames::CallbackMessageData;
use async_trait::async_trait;
use tokio::sync::RwLock;

/// 回调消息处理器
#[async_trait]
pub trait CallbackHandler: Send + Sync {
    async fn on_callback_message(
        &self,
        msg: CallbackMessageData,
        session_webhook: Option<String>,
    );
}

/// 生命周期监听器
pub trait LifecycleListener: Send + Sync {
    fn on_connected(&self) {}
    fn on_registered(&self) {}
    fn on_disconnected(&self) {}
    fn on_reconnecting(&self) {}
    fn on_error(&self, _error: &str) {}
}

/// 线程安全的处理器注册表（内部使用 RwLock）
pub struct HandlerRegistry {
    callback_handlers: RwLock<Vec<Box<dyn CallbackHandler>>>,
    lifecycle_listeners: RwLock<Vec<Box<dyn LifecycleListener>>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            callback_handlers: RwLock::new(Vec::new()),
            lifecycle_listeners: RwLock::new(Vec::new()),
        }
    }

    /// 注册回调消息处理器
    pub async fn register_callback(&self, handler: Box<dyn CallbackHandler>) {
        self.callback_handlers.write().await.push(handler);
    }

    /// 注册生命周期监听器
    pub async fn register_lifecycle(&self, listener: Box<dyn LifecycleListener>) {
        self.lifecycle_listeners.write().await.push(listener);
    }

    /// 分发回调消息
    pub async fn dispatch_callback(
        &self,
        msg: CallbackMessageData,
        session_webhook: Option<String>,
    ) {
        let handlers = self.callback_handlers.read().await;
        for handler in handlers.iter() {
            handler
                .on_callback_message(msg.clone(), session_webhook.clone())
                .await;
        }
    }

    /// 通知已连接
    pub fn notify_connected(&self) {
        let listeners = self.lifecycle_listeners.blocking_read();
        for l in listeners.iter() {
            l.on_connected();
        }
    }

    /// 通知已注册
    pub fn notify_registered(&self) {
        let listeners = self.lifecycle_listeners.blocking_read();
        for l in listeners.iter() {
            l.on_registered();
        }
    }

    /// 通知已断开
    pub fn notify_disconnected(&self) {
        let listeners = self.lifecycle_listeners.blocking_read();
        for l in listeners.iter() {
            l.on_disconnected();
        }
    }

    /// 通知正在重连
    pub fn notify_reconnecting(&self) {
        let listeners = self.lifecycle_listeners.blocking_read();
        for l in listeners.iter() {
            l.on_reconnecting();
        }
    }

    /// 通知发生错误
    pub fn notify_error(&self, error: &str) {
        let msg = error.to_string();
        let listeners = self.lifecycle_listeners.blocking_read();
        for l in listeners.iter() {
            l.on_error(&msg);
        }
    }
}
