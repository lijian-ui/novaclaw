pub mod session;
pub mod prompt;
pub mod cot;
pub mod runtime;

pub use runtime::AgentRuntime;
pub use session::AgentSession;

// 传入用户输入，执行完整 ReAct 循环，返回最终响应 AgentResult。
// 参考 claw-code `run_turn()` 模式实现。
