# 任务规划/任务分解系统实现分析报告

> 分析日期：2026-05-14
> 分析对象：hermes-agent / openclaw / codex

---

## 目录

1. [总体对比](#一总体对比)
2. [hermes-agent 分析](#二hermes-agent-分析)
3. [openclaw 分析](#三openclaw-分析)
4. [codex 分析](#四codex-分析)
5. [设计模式对比与总结](#五设计模式对比与总结)

---

## 一、总体对比

| 维度 | hermes-agent | openclaw | codex |
|------|-------------|----------|-------|
| **语言** | Python | TypeScript | Rust |
| **核心思路** | LLM 驱动的 Agent 委托（delegate_task） | 注册表驱动的任务/Flow 生命周期管理 | SessionTask trait + 多 Agent 线程树 |
| **任务分解方式** | `delegate_task` 工具（同进程子 Agent） | `sessions_spawn` 工具（子会话/ACP） | `spawn_agent` 控制平面（独立线程） |
| **深度控制** | 最大 3 层（配置可调） | 支持嵌套（orchestrator/leaf 角色） | Agent 树 `AgentPath` 深度不限 |
| **并行方式** | ThreadPoolExecutor | 独立 gateway 会话 | 独立 OS 线程 |
| **持久化** | 看板 SQLite（跨 profile） | TaskRegistry SQLite + FlowRegistry | AgentGraphStore SQLite（边持久化） |
| **中断机制** | 全局子 Agent 注册表 + 中断传播 | subagents(steer/kill) | `send_input`/`close_agent`/级联取消 |
| **任务追踪** | `todo` 工具 + 看板 | Task + Flow 双注册表 | SessionTask + TurnContext |
| **通知/结果** | 子 Agent 自动返回 | Task 投递策略（done/state/silent） | 事件流 + Rollout 持久化 |
| **技能系统** | SKILL.md 文档驱动（writing-plans, subagent-driven-dev） | TaskFlow 技能包 | core-skills 渲染层 |
| **工具规划** | ToolRegistry 动态 schema | `buildToolPlan()` 可用性评估 | ToolDef 框架 |
| **审查机制** | 两阶段（Implementer → Reviewer） | 内建 ReviewTask | `ReviewTask` 独立会话 |

---

## 二、hermes-agent 分析

### 2.1 整体架构

```
四层任务规划体系：

┌──────────────────────────────────────────────────────────────┐
│                    Skills（技能层）                             │
│  SKILL.md: writing-plans / subagent-driven-development        │
│  kanban-orchestrator / spec-compliance-reviewer               │
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│              Tools（工具层 - LLM 可调用）                      │
│  todo    → 当前会话内的任务列表管理                             │
│  delegate_task → 派生子 Agent（单任务 / 批量并行）              │
│  kanban_* → 跨会话/跨 Profile 的看板任务调度                    │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│             Agent（执行层 - AIAgent 实例）                     │
│  parent AIAgent → child AIAgent → grandchild AIAgent         │
│  每个 Agent 有独立上下文、终端、工具集、API 预算               │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│         Kanban 调度器（后台持久化调度层）                       │
│  SQLite DB + Dispatcher + Worker                             │
│  状态机: triage → todo → ready → running → blocked → done    │
│  跨 profile 共享、crash 检测、超时回收                        │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 核心文件索引

| 文件 | 大小 | 用途 |
|------|------|------|
| `tools/delegate_tool.py` | 2768 行 | **核心** — Agent 委托/子 Agent 架构 |
| `tools/todo_tool.py` | 278 行 | 任务列表管理工具 |
| `tools/registry.py` | ~600 行 | 中心工具注册表（动态 schema） |
| `toolsets.py` | — | 工具集定义（delegation/todo/kanban） |
| `hermes_cli/kanban_db.py` | 4000+ 行 | **核心** — 看板数据库 & 调度器 |
| `agent/prompt_builder.py` | — | 系统提示构建（含编排指导） |
| `skills/software-development/writing-plans/SKILL.md` | — | 编写实现计划的技能 |
| `skills/software-development/subagent-driven-development/SKILL.md` | — | 子 Agent 驱动开发技能 |
| `skills/devops/kanban-orchestrator/SKILL.md` | — | 看板编排器技能 |

### 2.3 核心数据结构

#### `delegate_task` 工具

```python
DELEGATE_TASK_SCHEMA = {
    "name": "delegate_task",
    "description": (
        "Create a subagent to handle a task independently. "
        "Can delegate a single task (goal) or multiple tasks in parallel (tasks array)."
    ),
    "parameters": {
        "goal": "str (optional)",           # 单任务模式
        "tasks": "[Task] (optional)",        # 批量并行模式
        "role": "'leaf' | 'orchestrator'",   # 子角色
        "toolsets": "[str]",                 # 子工具集
        "context": "str",                    # 上下文
        "max_concurrency": "int",            # 并发数
    }
}
```

- **阻止的工具**：`delegate_task`, `clarify`, `memory`, `send_message`, `execute_code`（防递归死循环）
- **默认深度**：1（仅 parent→child），最大可配至 3
- **角色**：`leaf`（不可再委托）/ `orchestrator`（可递归委托）

#### `TodoStore`

```python
class TodoStore:
    items: List[TodoItem]  # id, content, status (pending/in_progress/completed/cancelled)
    
    def replace(items):    # 替换模式 (merge=False)
    def update(items):     # 增量更新 (merge=True)
    def format_for_injection():  # 活跃任务注入
```

#### Kanban 看板

```python
# 任务卡片
{
    "id": "uuid",
    "title": "str",
    "description": "str",
    "status": "triage | todo | ready | running | blocked | done | archived",
    "parents": ["task_id"],       # 依赖
    "claim_lock", "claim_expires", # 原子声明
    "attempts", "max_attempts",
}

# 任务链接 (依赖关系)
task_links: { parent_id, child_id, kind: "blocks|depends_on" }

# 调度器
dispatch_once():
  1. 回收过期 claims
  2. 检测崩溃 workers
  3. 超时限制
  4. todo → ready（父依赖完成时）
  5. 为 ready 任务 spawn worker
```

### 2.4 Agent 委托执行流程

```
用户请求 → AIAgent 主循环
  │
  ├─ LLM 决定使用 todo 工具
  │   → 创建任务列表，追踪步骤
  │   → 完成任务后标记 completed
  │
  ├─ LLM 决定使用 delegate_task 工具
  │   ├─ _build_child_agent()
  │   │   ├─ 创建独立 AIAgent 实例
  │   │   ├─ 构建子系统提示（_build_child_system_prompt）
  │   │   ├─ 工具集继承 + 交集过滤
  │   │   └─ 注册到全局 _active_subagents
  │   │
  │   ├─ _run_single_child()（ThreadPoolExecutor）
  │   │   ├─ 心跳线程（防 gateway 超时）
  │   │   ├─ child.run_conversation()
  │   │   ├─ 硬超时 600 秒
  │   │   └─ 收集 token / 文件变更
  │   │
  │   └─ 返回结果给父 Agent
  │
  └─ LLM 决定使用 kanban_* 工具
      → 创建看板卡片
      → Dispatcher 后台自动调度
      → Worker 进程执行
      → 结果回写到看板
```

### 2.5 配置

```yaml
delegation:
  max_concurrent_children: 3       # 并行子 Agent 数
  max_iterations: 50               # 子 Agent 迭代预算
  max_spawn_depth: 1               # 深度 [1-3]
  orchestrator_enabled: true       # 编排器开关
  child_timeout_seconds: 600       # 超时
```

### 2.6 关键设计要点

1. **无 Plan Mode 开关**：规划通过 `todo` 和 `delegate_task` 的工具描述驱动，而非全局模式切换
2. **子 Agent 彻底隔离**：每个子 Agent 是完整 `AIAgent` 实例，独立上下文、终端、API 预算
3. **Kanban 跨 Profile 共享**：所有 profile 共享同一个 SQLite 看板数据库
4. **动态 Schema 覆盖**：`delegate_task` 的描述在每次调用时根据运行时配置动态生成，LLM 看到的是实际限制

---

## 三、openclaw 分析

### 3.1 整体架构

```
三层注册表体系：

┌──────────────────────────────────────────────────────────────┐
│                TaskFlowRegistry（流程注册表）                   │
│  生命周期: queued → running → waiting/blocked → succeeded     │
│  模式: task_mirrored（自动同步）/ managed（代码控制）          │
│  乐观锁: revision 字段防冲突                                  │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│                TaskRegistry（任务注册表）                      │
│  记录: taskId, runtime, status, ownerKey, childSessionKey    │
│  运行时: subagent / acp / cli / cron                         │
│  存储: 内存 Map + SQLite 持久化                               │
│  索引: 多维度（runId/ownerKey/parentFlowId）                  │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│             SubagentRegistry（子代理注册表）                   │
│  记录子会话关系、清理策略（delete/keep）、嵌套深度              │
│  角色: orchestrator（可继续派发）/ leaf（末端执行）             │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 核心文件索引

| 文件 | 大小 | 用途 |
|------|------|------|
| `src/tasks/task-registry.ts` | 2173 行 | **核心** — 任务注册表 |
| `src/tasks/task-executor.ts` | 697 行 | 任务执行器 |
| `src/tasks/task-flow-registry.ts` | 706 行 | Flow 注册表 |
| `src/agents/tools/sessions-spawn-tool.ts` | 465 行 | **核心** — 子任务生成工具 |
| `src/agents/tools/subagents-tool.ts` | 185 行 | 子 Agent 管理(list/kill/steer) |
| `src/agents/subagent-spawn.ts` | — | 子 Agent 生成逻辑 |
| `src/agents/subagent-system-prompt.ts` | — | 子 Agent 系统提示 |
| `src/agents/system-prompt.ts` | — | 主系统提示（含编排指导） |
| `src/tools/planner.ts` | — | 工具规划器（可用性评估） |
| `skills/taskflow/SKILL.md` | — | TaskFlow 技能使用指南 |

### 3.3 核心数据结构

#### TaskRecord（任务记录）

```typescript
type TaskRecord = {
  taskId: string;                    // 唯一 ID
  runtime: "subagent" | "acp" | "cli" | "cron";  // 运行时类型
  requesterSessionKey: string;       // 请求方会话
  ownerKey: string;                  // 所有者
  childSessionKey?: string;          // 子会话（子代理/ACP）
  parentFlowId?: string;             // 所属 Flow
  parentTaskId?: string;             // 父任务（重试链）
  agentId?: string;                  // 目标 Agent
  task: string;                      // 任务描述
  status: "queued" | "running" | "succeeded" | "failed" 
        | "timed_out" | "cancelled" | "lost";  // 7 种状态
  deliveryStatus: TaskDeliveryStatus;
  notifyPolicy: TaskNotifyPolicy;    // 通知策略
};
```

#### TaskFlowRecord（流程记录）

```typescript
type TaskFlowRecord = {
  flowId: string;
  syncMode: "task_mirrored" | "managed";  // 同步模式
  status: "queued" | "running" | "waiting" | "blocked" 
        | "succeeded" | "failed" | "cancelled" | "lost";  // 8 种状态
  goal: string;                      // 目标
  currentStep?: string;              // 当前步骤
  blockedTaskId?: string;            // 阻塞的任务 ID
  revision: number;                  // 乐观锁版本号
  stateJson?: JsonValue;             // 持久化状态
  waitJson?: JsonValue;              // 等待元数据
};
```

#### SubagentRunRecord（子代理记录）

```typescript
type SubagentRunRecord = {
  runId: string;
  childSessionKey: string;
  task: string;
  cleanup: "delete" | "keep";        // 清理策略
  // 嵌套编排
  parentRunId?: string;              // 父运行 ID
  spawnDepth?: number;               // 嵌套深度
  subagentRole?: "orchestrator" | "leaf";
};
```

### 3.4 子 Agent 生成执行流程

```
主 Agent 会话
  │
  ├─ 调用 sessions_spawn(task, runtime="subagent")
  │
  ├─ createTaskRecord (TaskRegistry: queued)
  ├─ createTaskFlowForTask (FlowRegistry: task_mirrored)
  │
  ├─ spawnSubagentDirect
  │   ├─ resolveSubagentModelAndThinkingPlan
  │   ├─ prepareSubagentSessionContext
  │   │   ├─ "isolated": 空上下文
  │   │   └─ "fork": 继承父会话上下文
  │   ├─ buildSubagentSystemPrompt
  │   │   └─ 含编排规则（orchestrator 可再派发）
  │   ├─ registerSubagentRun (SubagentRegistry)
  │   └─ callGateway → 启动子 Agent 会话
  │
  ├─ 子 Agent 运行 → 自动投递结果
  │
  └─ Task 完成 → TaskRegistry 更新状态
```

### 3.5 Managed Flow 生命周期

```
1. createManaged({ controllerId, goal, stateJson })
2. runTask({ flowId, runtime, task, ... })
3. setWaiting({ currentStep, stateJson, waitJson })  // 等待外部输入
4. resume({ status: "running", ... })
5. finish({ stateJson }) 或 fail({ ... })
6. requestCancel / cancel
```

所有有状态的修改都使用 `expectedRevision` 进行乐观锁冲突检测。

### 3.6 工具规划器（ToolPlan）

```typescript
type ToolPlan = {
  visible: ToolPlanEntry[];       // 可见工具（有 executor）
  hidden: HiddenToolPlanEntry[];  // 隐藏工具（带诊断信息）
};

buildToolPlan():
  1. 排序工具描述符（sortKey/name）
  2. 检查唯一名称
  3. 评估每个工具的可用性
  4. 不可用的放入 hidden（带原因）
  5. 可见工具必须有 executor
```

### 3.7 关键发现

1. **双注册表设计**：TaskRegistry（单体任务） + TaskFlowRegistry（流程编排），职责分离
2. **ACP 协议支持**：`sessions_spawn` 支持 `acp` 运行时，跨机器 Agent 通信
3. **上下文模式**：子 Agent 可选择 `isolated`（隔离）或 `fork`（继承父上下文）
4. **乐观锁**：Flow 状态修改使用 revision 版本号防冲突
5. **通知策略**：三种通知级别（done_only / state_changes / silent）
6. **分离任务**：插件可注册自定义运行时（通过 `DetachedTaskLifecycleRuntime` 接口）

---

## 四、codex 分析

### 4.1 整体架构

```
Agent 线程树 + SessionTask 框架：

┌──────────────────────────────────────────────────────────────┐
│                    Session（会话）                              │
│  SessionTask trait → RegularTask / CompactTask / ReviewTask  │
│  spawn_task / abort_all_tasks / start_task / on_task_finished│
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│               Agent Control（Agent 控制平面）                  │
│  spawn_agent_with_metadata / send_input / close_agent        │
│  Agent 树: Thread Tree — 父子关系 → Edge 持久化              │
│  级联关闭: 父关闭 → 所有在线后代关闭                          │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│               Agent Registry（Agent 注册表）                   │
│  Agent 树: HashMap<ThreadId, AgentMetadata>                  │
│  角色: explorer / worker / default                           │
│  总数限制: AtomicUsize                                       │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────────┐
│            Agent Graph Store（图谱持久化）                     │
│  SQLite: upsert_thread_spawn_edge / list_children             │
│  边状态: active / completed / failed / orphaned              │
│  用于恢复和监控                                              │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 核心文件索引

| 文件 | 用途 |
|------|------|
| `codex-rs/core/src/tasks/mod.rs` | **核心** — SessionTask trait 定义 |
| `codex-rs/core/src/tasks/regular.rs` | RegularTask — 标准对话轮次 |
| `codex-rs/core/src/tasks/compact.rs` | CompactTask — 对话压缩 |
| `codex-rs/core/src/tasks/review.rs` | ReviewTask — 独立审查子会话 |
| `codex-rs/core/src/tasks/user_shell.rs` | UserShellCommandTask — Shell 命令 |
| `codex-rs/core/src/agent/control.rs` | **核心** — Agent 控制平面 |
| `codex-rs/core/src/agent/registry.rs` | Agent 注册表 |
| `codex-rs/agent-graph-store/` | Agent 图谱 SQLite 持久化 |
| `codex-rs/collaboration-mode-templates/` | 协作模式提示词模板 |
| `codex-rs/core/src/session/session.rs` | 会话生命周期（spawn_task） |

### 4.3 核心数据结构

#### SessionTask trait

```rust
pub(crate) trait SessionTask: Send + Sync + 'static {
    fn kind(&self) -> TaskKind;
    fn span_name(&self) -> &'static str;
    async fn run(
        self: Arc<Self>,
        session: Arc<SessionTaskContext>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;
    async fn abort(&self, session: Arc<SessionTaskContext>, ctx: Arc<TurnContext>);
}
```

#### AgentControl

```rust
pub struct AgentControl {
    session_id: SessionId,
    manager: Weak<ThreadManagerState>,
    state: Arc<AgentRegistry>,
}

impl AgentControl {
    pub fn spawn_agent_with_metadata(
        &self, spawn_request: SpawnRequest, metadata: AgentMetadata
    ) -> Result<ThreadId, AgentSpawnError>;
    pub fn send_input(&self, agent_id: ThreadId, input: UserInput) -> Result<(), AgentControlError>;
    pub fn send_inter_agent_communication(...);
    pub fn close_agent(&self, agent_id: ThreadId, reason: &str);
    pub fn list_agents(&self) -> Vec<AgentSummary>;
}
```

#### AgentMetadata（Agent 树节点）

```rust
pub(crate) struct AgentMetadata {
    agent_id: Option<ThreadId>,
    agent_path: Option<AgentPath>,      // 层级路径 /root/task1/subtask
    agent_nickname: Option<String>,     // 可读昵称
    agent_role: Option<String>,         // explorer / worker / default
    last_task_message: Option<String>,
}
```

#### SpawnRequest

```rust
pub struct SpawnRequest {
    pub agent_path: AgentPath,  // 层级路径，如 /root/explore-task
    pub agent_role: Option<String>,
    pub session_id: SessionId,
    pub provider_name: Option<String>,
    pub model_name: Option<String>,
    pub system_prompt: String,
    pub auto_approve_prompt_injection: bool,
}
```

### 4.4 Agent 线程树执行流程

```
用户输入
  │
  ├─ Session::spawn_task(RegularTask)
  │   ├─ SessionTask::run() → run_turn() 循环
  │   ├─ LLM 生成 → 检查系统提示中的 agent 工具
  │   │
  │   ├─ LLM 调用 spawn_agent 工具
  │   │   ├─ 解析 SpawnRequest（role/model/system_prompt）
  │   │   ├─ AgentControl::spawn_agent_with_metadata()
  │   │   │   ├─ 创建新 OS 线程
  │   │   │   ├─ 注册到 AgentRegistry（设置 agent_path）
  │   │   │   ├─ 持久化边到 AgentGraphStore
  │   │   │   └─ 启动子 Session + SessionTask
  │   │   │
  │   │   ├─ 子 Agent 独立运行
  │   │   │   ├─ 可进一步 spawn 子 Agent（递归）
  │   │   │   └─ 完成后通过 Rollout 持久化结果
  │   │   │
  │   │   └─ 父 Agent 通过 send_input 与子 Agent 通信
  │   │
  │   ├─ LLM 调用 send_inter_agent_communication
  │   │   └─ 子 Agent 收到消息并处理
  │   │
  │   ├─ 关闭子 Agent → close_agent(agent_id, reason)
  │   │   └─ 级联关闭所有后代
  │   │
  │   └─ 会话结束 → 所有 Agent 关闭
```

### 4.5 ReviewTask（审查子会话）

```rust
// 独立于主会话运行的审查任务
// 启动一个子 Agent 进行代码审查
// 审查结果作为 ReviewOutputEvent 输出
// 审查 Agent 不参与主对话，专注于代码分析
pub(crate) struct ReviewTask {
    review_type: ReviewType,
    scope: ReviewScope,
}
```

### 4.6 协作模式提示词模板

`collaboration-mode-templates/` 目录包含多 Agent 协作的系统提示词模板：

```rust
pub enum CollaborationMode {
    /// 一个核心 worker + 多个独立 explorer，并行探索后汇总
    Research,  
    /// 一个 orchestrator 管理多个 worker，分发并汇总
    Orchestrated,
    /// 接力模式：worker → reviewer → approver 链式处理
    AssemblyLine,
}
```

**Research 模式流程**：
1. Explorer Agent 并行探索
2. Worker Agent 聚合结果
3. 用户查看合并结果

### 4.7 关键设计要点

1. **SessionTask trait**：统一的任务抽象，4 种内置实现（Regular/Compact/Review/UserShell）
2. **OS 线程级隔离**：每个子 Agent 在独立线程运行，彻底隔离
3. **Agent 树 + 级联关闭**：父子关系持久化到 SQLite，关闭时级联取消所有后代
4. **Rollout 持久化**：子 Agent 结果通过 Rollout 机制持久化，支持断点恢复
5. **协作模式**：三种预定义多 Agent 协作模式（Research/Orchestrated/AssemblyLine）
6. **ReviewTask 独立会话**：审查作为独立 SessionTask 运行，不阻塞主流程

---

## 五、设计模式对比与总结

### 5.1 任务分解范式

```
┌──────────────────┐   ┌──────────────────┐   ┌──────────────────┐
│  Agent 委托式     │   │  注册表驱动式     │   │  线程树驱动式    │
│  (hermes-agent)  │   │  (openclaw)      │   │  (codex)         │
│                  │   │                  │   │                  │
│  LLM 自主调用     │   │  强类型注册表     │   │  Rust trait 抽象  │
│  delegate_task   │   │  TaskRegistry    │   │  SessionTask     │
│  同进程子 Agent   │   │  SQLite 持久化    │   │  OS 线程隔离      │
│  线程池并行       │   │  乐观锁防冲突     │   │  Agent 树级联    │
│  Kanban 跨会话   │   │  ACP 跨机器      │   │  Rollout 持久化  │
└──────────────────┘   └──────────────────┘   └──────────────────┘
```

### 5.2 关键设计决策对比

| 决策 | hermes-agent | openclaw | codex |
|------|-------------|----------|-------|
| **任务分解执行者** | LLM 通过工具调用分解 | LLM 通过 sessions_spawn 工具分解 | Agent 通过控制平面 API 分解 |
| **子任务隔离** | 同进程 AIAgent 实例 | 独立 gateway 会话 | 独立 OS 线程 |
| **深度控制** | 最大 3 层（配置） | 支持嵌套（orchestrator/leaf） | Agent 树深度不限 |
| **持久化** | 看板 SQLite | Task + Flow 双 SQLite | 图谱边 SQLite |
| **中断机制** | 全局注册表 + 中断传播 | subagents(steer/kill) | send_input / 级联 close |
| **结果传递** | 子 Agent 自动返回 | Task 投递 + 通知策略 | Rollout 事件流 |
| **并发控制** | ThreadPoolExecutor | 独立 Gateway 会话 | OS 线程 + 注册表限制 |
| **审查机制** | 两阶段（Implementer→Reviewer） | 内建 ReviewTask | ReviewTask 独立会话 |
| **工具规划** | 动态 schema 覆盖 | buildToolPlan 可用性评估 | ToolDef 框架 |
| **学习/进化** | 无 | 无 | 无 |

### 5.3 对 NovaClaw 的启示

1. **子任务工具是刚需**：三个项目都通过 spawn/subagent/delegate 工具让 LLM 自主分解任务。NovaClaw 缺少这类工具。

2. **隔离 vs 共享**：
   - codex 提供最强隔离（OS 线程），但开销大
   - hermes-agent 同进程隔离，轻量但风险高
   - openclaw 独立会话，平衡了隔离和开销

3. **持久化是关键**：openclaw 的 Task + Flow 双注册表设计最适合 NovaClaw 的场景——任务状态持久化到 SQLite，支持恢复和监控。

4. **中断机制**：三个项目都实现了不同程度的中断（steer/kill/close），NovaClaw 目前缺少这个能力。

5. **深度控制**：hermes-agent 和 openclaw 都有深度限制（最大 3 层），codex 不限深度但有级联关闭。NovaClaw 应当加深度限制防递归失控。

6. **工具规划**：openclaw 的 `buildToolPlan()` —— 动态过滤不可用工具并告警 —— 对限制子 Agent 能力范围很有价值。
