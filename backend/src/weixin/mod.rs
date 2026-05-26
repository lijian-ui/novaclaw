//! 微信个人号 IM 集成模块
//!
//! 基于腾讯官方 iLink Bot API 实现个人微信消息收发。
//! 协议：HTTP REST + 长轮询，无需 WebSocket 和公网服务器。
//!
//! ## 架构
//!
//! ```
//! WeixinClient (client.rs) → HTTP 长轮询 → IMGateway → Agent
//!      ↓
//! send_message (HTTP POST) ← Agent 回复
//! ```
//!
//! ## 使用前提
//!
//! 1. 微信小号扫码登录获取 bot_token
//! 2. 配置 config/im.json 中的 weixin 账号
//!
//! ## 限制
//!
//! - 不支持 Markdown 渲染（自动转纯文本）
//! - 单条消息最长 4000 token
//! - 不支持图片/文件消息（一期）

pub mod adapter;
pub mod client;

pub use adapter::WeixinAdapter;
pub use client::WeixinClient;
