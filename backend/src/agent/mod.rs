pub mod session;
pub mod prompt;
pub mod cot;
pub mod runtime;
pub mod task;
pub mod task_detector;

pub use runtime::AgentRuntime;
pub use session::AgentSession;
pub use task::{SubTask, TaskPlan, TaskDAG, TaskProgress, TaskDecompositionResult};
pub use task_detector::{TaskComplexityDetector, DetectionResult};

// 传入用户输入，执行完整 ReAct 循环，返回最终响应 AgentResult。
// 参考 claw-code `run_turn()` 模式实现。
