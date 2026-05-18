# `delete_file` 确认功能 — 代码审查报告

> 审查日期：2026-05-18
> 审查范围：backend/src/tools/approval.rs, types.rs, registry.rs, agent/runtime.rs, server/routes/chat.rs
> 关联文档：[命令执行确认机制设计规范](./命令执行取认机制.md)
> 缺陷跟踪：[BUG修复与性能优化任务列表](./BUG修复与性能优化任务列表.md)

---

## 审查结论概览

| 维度 | 评分 | 说明 |
|------|------|------|
| 设计符合度 | ⚠️ 部分符合 | 架构设计采纳了，但实际实现有偏差 |
| 逻辑正确性 | 🟡 基本正确 | 核心流程通，但有 2 个 P0 级缺陷 |
| 安全性 | 🔴 存在隐患 | 路径穿越防护有 TOCTOU 竞赛条件 |
| 用户体验 | 🟢 良好 | SSE 流式 + 自动继续执行体验完整 |
| 错误处理 | 🟡 基本覆盖 | 边缘场景有遗漏 |
| 代码质量 | 🟡 可改进 | approve_tool 函数过长，重复代码多 |

共发现 **12 个问题**（P0×2, P1×4, P2×4, P3×2）。

---

## 一、设计符合度检查

### 1.1 设计文档的核心要求 vs 实际实现

| 设计文档要求 | 实际实现 | 状态 |
|-------------|---------|------|
| `ToolResult` 枚举含 `PendingApproval` 变体 | `types.rs:47` — 已实现 | ✅ |
| `ApprovalRequired` 含 `affected_files` | `types.rs:31` — 已实现但**从未被填充** | ⚠️ |
| `AgentStep` 含 `approval` + `approval_id` | `types.rs:56` — 已实现 | ✅ |
| `ApprovalManager` 全局管理待确认 | `approval.rs:92` — 已实现 | ✅ |
| 工具 handler 返回 `PendingApproval` | **builtin.rs 无 delete_file handler**，未实现 | 🔴 |
| Agent Runtime 识别并分发确认事件 | `runtime.rs:414` — 已实现 | ✅ |
| 前端显示确认对话框 | `ChatPanel.tsx` — 已实现 | ✅ |
| `/chat/approve` 流式端点 | `chat.rs:510` — 已实现 | ✅ |
| 确认后自动继续 Agent 执行 | `chat.rs:647` — 已实现 | ✅ |
| 5 分钟超时自动清理 | `approval.rs:145` — 已实现 | ✅ |

### 1.2 设计偏差

**P2 - `affected_files` 字段从未被填充**  
`ApprovalRequired.affected_files` 字段已定义，但 `agent/runtime.rs:414-451` 在处理 `PendingApproval` 时，`approval` 是从工具 handler 返回的，而 handler 从未填充 `affected_files`。前端本可以用它列出受影响的文件列表。

**P2 - 设计文档提到配置化"哪些工具需要确认"**  
当前所有工具 handler 必须手动返回 `PendingApproval`，没有集中式的配置来控制某个工具是否需要确认。如果要扩展确认范围，需要修改每个 handler。

---

## 二、逻辑错误与安全隐患

### 2.1 🔴 P0: `canonicalize()` 不存在的路径导致安全降级

**位置**：`tools/approval.rs:26-35`

```rust
if let Ok(canon_base) = base_dir.canonicalize() {
    if let Ok(canon_resolved) = resolved.canonicalize() {
        if canon_resolved.starts_with(&canon_base) {
            return canon_resolved;
        }
        // 路径越界，退回安全边界
        tracing::warn!("[Security] 路径穿越尝试被阻止: {} → {}", path_str, canon_resolved.display());
        return canon_base;
    }
}

// 降级到字符串前缀判断
let base_str = base_dir.to_string_lossy().to_string();
let resolved_str = resolved.to_string_lossy().to_string();
if resolved_str.starts_with(&base_str) {
    return resolved;
}
```

**问题**：`canonicalize()` 要求文件/目录**必须存在**。在 `delete_file` 场景下，`resolve_path` 可能在删除之前被调用（生成 `affected_files`），此时路径存在于磁盘上。但如果在删除那一刻被第二次调用（在 `execute_delete_file` 通过 `resolve_path` 解析路径），文件已经被其他进程删除，`canonicalize()` 会失败，**退化到字符串前缀检查**。

**字符串前缀检查的绕过方式**：
```
base_dir = D:\workspace\
path     = D:\workspace\..\..\windows\system32\evil.exe
resolved = D:\workspace\..\..\windows\system32\evil.exe  // 没有 canonicalize，保留 ..
```
经 `.join()` 后 `resolved_str` = `D:\workspace\..\..\windows\system32\evil.exe`，确实以 `base_str` 开头，因为真实的规范化路径是 `D:\windows\system32\evil.exe`。

**建议修复**：
1. 在 `resolve_path` 返回前，对路径做一次 `fs::canonicalize()` 强制性检查
2. 如果 canonicalize 失败，尝试对基础目录做 canonicalize，然后手动解析 `..` 组件
3. 或使用 `dunce::simplified()` + 手动处理 `..` 组件

### 2.2 🔴 P0: TOCTOU 竞赛条件 — 路径检查与删除之间的窗口

**位置**：`tools/approval.rs:9-46`（resolve_path）→ `tools/approval.rs:69-77`（实际删除）

```
时间 T1: resolve_path() 检查路径安全 → 返回 resolved_path
时间 T2: (攻击者将 resolved_path 替换为符号链接到敏感文件)
时间 T3: remove_file(resolved_path) → 实际删除的是符号链接的目标
```

这是一个经典的 **Time-of-Check / Time-of-Use (TOCTOU)** 漏洞。虽然 resolve_path 检查了路径安全性，但在检查完成和实际删除之间，文件系统状态可能已改变。

**影响**：攻击者如果能写入用户 workspace（或通过其他工具创建符号链接），可能诱导 delete_file 删除非预期的文件。

**建议修复**：
1. 在删除前再次 canonicalize 已解析的路径
2. 使用 `open()` + ` open_dir()` 获取文件句柄（而非路径字符串），通过句柄删除
3. 最低成本方案：删除前重新检查 `resolved.canonicalize()` 是否仍然在 `base_dir` 范围内

### 2.3 🟡 P1: approve_tool 中 `drop(state)` 后使用 `session_store`

**位置**：`chat.rs:536-543`

```rust
let (config, models_config, tool_registry, skills, session_store) = (
    state.config.clone(),
    state.models_config.clone(),
    state.tool_registry.clone(),
    state.skills_loader.list_skills(),
    state.session_store.clone(),
);
drop(state);
```

然后后续又出现了新的 `APP_STATE.read().await` 调用（第 578 行）。`SessionStore` 是文件操作，没有内部缓存，所以这是安全的。但 `drop(state)` 后的重新加锁（`APP_STATE.write().await` 在第 557、619、747 行）可能导致**死锁风险**——如果其他代码持有写锁且正在等待读锁。

**建议**：在使用 `session_store` 的地方统一使用克隆后的实例，不要重新读取 APP_STATE。

### 2.4 🟡 P1: approve_tool 未防止并发重复确认

**位置**：`chat.rs:521-533, chat.rs:618-621`

```rust
let pending = match state.approval_manager.get_pending_full(&req.approval_id).await {
    Some(p) => p,
    None => { /* 返回错误 */ return; }
};

// ... 大量处理代码 ...

// 清理待确认
let mut state = APP_STATE.write().await;
state.approval_manager.remove_pending(&req.approval_id).await;
```

**问题**：如果在 `get_pending_full` 和 `remove_pending` 之间，另一个并发请求处理了同一个 `approval_id`，两个请求都会执行相同操作，导致**重复删除**。

**建议**：采用「先移除后执行」模式。先将 pending 记录从 HashMap 移除（`remove` 返回被移除的值），然后再根据移除的值执行操作。如果 `remove` 返回 `None`，说明已经被处理过。

### 2.5 🟡 P1: cancel_map 复合 key 可能被不同 approval_id 覆盖

**位置**：`chat.rs:745`

```rust
let cancel_key = format!("approve:{}:{}", req.session_id, req.approval_id);
{
    let mut state = APP_STATE.write().await;
    state.cancel_map.insert(cancel_key.clone(), cancel_flag.clone());
}
```

如果同一个 session 中存在多个待确认操作，快速点击确认两个不同 approval 时，第二个会覆盖第一个的 cancel key。虽然概率低，但 `cancel_map` 从未被清理可能造成内存泄漏。

**建议**：在 `remove_pending` 时同步清理对应的 cancel_map 条目。

---

## 三、用户体验评估

### 3.1 🟢 P3: 前端收到 approval_required 事件到显示对话框的延迟

**位置**：`agent/runtime.rs:414-451` → SSE → 前端

从 agent 返回 `PendingApproval` → 通过 `step_tx` 发送 → `chat_stream` 中的前向任务转换为 SSE → 前端收到 → 渲染对话框。这个流程已经经过了实测，延迟在合理范围内。

### 3.2 🟡 P2: 确认对话框只显示 `message`，缺少 `affected_files`

**位置**：`runtime.rs:437`

```rust
AgentStep {
    step_type: "approval_required".to_string(),
    content: approval.message.clone(),  // 只有文字描述
    ...
    approval: Some(approval),           // 含 affected_files
}
```

`approval` 对象（含 `affected_files`）被原样发送到了前端（`chat.rs:314-325`），但**前端从未使用 `affected_files` 字段**。对话框可以展示具体的文件名列表来帮助用户做判断。

### 3.3 🟢 P3: 自动继续执行体验

`chat.rs:647-764` 的自动继续执行流程实现了完整的体验：确认 → 执行工具 → 流式输出结果 → 自动继续 LLM → 继续流式输出。这是设计文档中描述的"全自动"行为，体验良好。

---

## 四、错误处理与边界条件

### 4.1 🟡 P1: session_id 验证使用 `!=` 而非类型化比较

**位置**：`chat.rs:546`

```rust
if session_id_from_pending != req.session_id {
```

两个 `String` 的 `!=` 比较是大小写敏感的。如果前端有 URL 编码或大小写不一致的问题，会导致验证失败。建议统一使用 `eq` 或 `==` 并记录日志。

### 4.2 🟡 P2: 确认后恢复 Agent 时的消息历史不完整

**位置**：`chat.rs:690-695`

```rust
let history = session_store.get_messages(&req.session_id).unwrap_or_default();
for m in &history {
    if m.role != "system" {
        agent_session.push_message(storage_msg_to_agent_msg(m));
    }
}
```

**问题**：从存储中恢复的消息不包含 `tool_calls` 信息（`storage_msg_to_agent_msg` 会转换，但存储的 Message 是否包含 `tool_calls`？）。如果之前的 LLM 回复中有工具调用，这些信息在恢复时可能丢失，导致 LLM 在 continue_prompt 中看到不完整的上下文。

**检查**：`storage::Message` 结构体（`storage.rs:37`）包含 `tool_calls: Option<Vec<ToolCall>>` 字段，`storage_msg_to_agent_msg` 会转换。存储逻辑在 `chat_stream` 中是否保存了 `tool_calls`？需要在 `chat_stream` 的任务中确认。

### 4.3 🟢 P2: approve_tool 中 session 恢复后缺少 workspace 注入

**位置**：`chat.rs:682-687`

```rust
let mut agent_session = AgentSession::new(
    &existing_session.name,
    &model,
    existing_session.metadata.as_deref()  // workspace
);
```

`AgentSession::new` 的第三个参数是 `workspace`。`metadata` 字段可能不包含 workspace 信息（取决于创建时的元数据格式）。如果 workspace 为 `None`，后续工具执行的路径解析会使用默认全局 workspace，导致路径错误。

---

## 五、代码注释与文档

### 5.1 🟢 注释充分性

- `tools/approval.rs` — 函数级注释基本完整，`resolve_path` 有安全相关的注释
- `tools/registry.rs` — 熔断器状态机有完整的中文注释
- `agent/runtime.rs:414-451` — `PendingApproval` 处理分支有清晰的日志
- `chat.rs:647` — "自动继续 Agent 执行"有注释分隔

### 5.2 🟡 P2: `resolve_path` 的安全注释与实际风险不匹配

```rust
// 路径穿越防护：规范化后校验路径是否在允许范围内
```

注释只提到「路径穿越」，没有提到 TOCTOU 竞赛条件和 canonicalize 失败降级的风险。建议补充注释说明安全降级行为和 TOCTOU 风险。

### 5.3 🟢 P3: 缺少对新开发者友好的模块级文档

`tools/approval.rs` 文件顶部没有模块级文档注释，新开发者需要通读代码才能理解 `resolve_path` / `execute_delete_file` / `ApprovalManager` 三者的关系。

---

## 六、问题汇总与优先级排序

| ID | 等级 | 文件 | 行号 | 描述 | 建议 |
|----|------|------|------|------|------|
| CR-01 | 🔴 P0 | `approval.rs` | 26-46 | `canonicalize()` 失败降级到字符串前缀检查，可被 `..` 绕过 | 对 resolved 路径做手动规范化 |
| CR-02 | 🔴 P0 | `approval.rs` | 9→77 | TOCTOU：路径检查与删除间可被符号链接攻击 | 删除前重新 canonicalize |
| CR-03 | 🟡 P1 | `chat.rs` | 522-621 | 并发重复确认导致重复删除 | 先 remove 后执行 |
| CR-04 | 🟡 P1 | `chat.rs` | 543 | `drop(state)` 后多次重读 APP_STATE，死锁风险 | 统一使用克隆后的资源 |
| CR-05 | 🟡 P1 | `chat.rs` | 745 | cancel_map 复合 key 可能被覆盖 + 内存泄漏 | 确认处理后清理 cancel_map |
| CR-06 | 🟡 P1 | `runtime.rs` | 414-451 | `affected_files` 从未填充 | 在 handler 中生成文件列表 |
| CR-07 | 🟡 P2 | `chat.rs` | 690-695 | 确认后恢复的会话可能丢失工具调用上下文 | 验证存储流程包含 tool_calls |
| CR-08 | 🟡 P2 | `chat.rs` | 682-687 | workspace 可能未正确恢复 | 使用原始 session 的 workspace |
| CR-09 | 🟡 P2 | `approval.rs` | 全部 | `resolve_path` 同时被 approval 前和 approval 后调用，语义不同 | 拆分为 resolve_for_check / resolve_for_exec |
| CR-10 | 🟢 P2 | `types.rs` | 31-43 | `approval.operation_type` 是 `"delete_file"` 而非设计文档的 `"delete"` | 统一常量 |
| CR-11 | 🟢 P3 | `approval.rs` | 145 | 5 分钟超时可能不够，无用户配置接口 | 可配置化超时时间 |
| CR-12 | 🟢 P3 | `approval.rs` | 1 | 缺少模块级文档 | 增加 `//!` 模块注释 |

---

## 七、修复建议优先级

### 立即修复（P0）

1. **CR-01 + CR-02 路径安全**：重写 `resolve_path`，添加手动路径规范化逻辑，不依赖 `canonicalize()`
2. **CR-03 并发安全**：`ApprovalManager` 改用 `HashMap::remove()` 替代 `get_pending_full()` + `remove_pending()` 两个步骤

### 本迭代修复（P1）

3. **CR-04**：重构 `approve_tool`，避免 `drop(state)` 后的重复加锁
4. **CR-05**：在处理完成后主动清理 `cancel_map`
5. **CR-06**：在 `delete_file` handler 中填充 `affected_files`

### 排期修复（P2）

6. **CR-07 + CR-08**：完善 approve 恢复流程
7. **CR-09**：分离 resolve_path 的两种使用场景

### 后续优化（P3）

8. **CR-11**：超时时间可配置
9. **CR-12**：补充模块文档
