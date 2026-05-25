//! 统一 IM 渠道抽象层
//!
//! 提供跨平台 IM 消息收发的统一抽象。
//! 各渠道适配器（DingTalk、Slack 等）实现 `IMAdapter` trait，
//! 注册到 `AccountRegistry`，通过 `IMGateway` 统一管理。
//!
//! ## 架构
//!
//! ```text
//! IMGateway (gateway.rs)
//!   └─ AccountRegistry (registry.rs) ─ HashMap<accountId, Arc<dyn IMAdapter>>
//!        ├─ DingTalkAdapter (dingtalk/adapter.rs) → DingTalkClient (多账号)
//!        ├─ ... (未来: SlackAdapter → Slack SDK)
//!        └─ ... (未来: WeChatWorkAdapter → 企微 SDK)
//! ```

pub mod adapter;
pub mod config;
pub mod gateway;
pub mod handler;
pub mod registry;
pub mod reload;
pub mod session;
pub mod types;

pub use adapter::IMAdapter;
pub use gateway::IMGateway;
pub use registry::AccountInfo;
pub use registry::AccountRegistry;
pub use types::SessionSource;
pub use types::*;
