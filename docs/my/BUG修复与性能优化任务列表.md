# BUG 修复与性能优化 — 任务列表

> **生成日期**：2026-05-15
> **维护人**：开发团队
> **关联文档**：[命令执行确认机制设计规范](./命令执行取认机制.md)

---

## 目录

- [一、严重等级定义](#一严重等级定义)
- [二、BUG 列表](#二bug-列表)
- [三、性能优化列表](#三性能优化列表)
- [四、代码质量改进列表](#四代码质量改进列表)
- [五、任务执行计划（含依赖关系）](#五任务执行计划含依赖关系)
- [六、验收标准汇总](#六验收标准汇总)

---

## 一、严重等级定义

| 等级 | 标识 | 定义 | 响应要求 |
|------|------|------|----------|
| **P0** | 🔴 严重 | 核心功能错误、数据安全隐患、可能导致服务崩溃 | 立即修复，阻塞发布 |
| **P1** | 🟡 中等 | 功能缺陷、资源泄漏、可靠性风险 | 本迭代内修复 |
| **P2** | 🟢 一般 | 用户体验问题、代码规范问题 | 排期修复 |
| **P3** | ⚪ 建议 | 优化建议、非紧急改进 | 可延后 |

---

## 二、BUG 列表

### BUG-001 🔴 P0: `execute_delete_file` 中 workspace 路径解析缺陷 ✅ 已修复

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/approval.rs` `resolve_path`、`backend/src/server/routes/chat.rs` `approve_tool` |
| **发现来源** | 代码审查 — 数据流追踪 |
| **影响范围** | 所有使用会话级自定义 workspace + 相对路径的 `delete_file` 确认操作 |
| **状态** | ✅ 已修复（2026-05-18） |
| **预估工时** | 30 分钟 |

**问题描述：**

`approve_tool` 中调用 `execute_delete_file(args, None)` 时传入 `workspace=None`。此时 args 是从 `add_pending` 阶段保存的原始 LLM 工具调用参数（不含 `_workspace` 注入），导致 `resolve_path` 回退到全局默认 workspace 而非当前会话的 workspace。

**复现步骤：**

1. 创建一个使用自定义 `workspace`（如 `D:\custom_ws`）的 Agent 会话
2. 请求 LLM 删除一个相对路径文件（如 `temp\test.txt`，该文件位于 `D:\custom_ws\temp\test.txt`）
3. 在弹出的确认对话框中点击"确认执行"
4. 观察实际删除的是全局默认 workspace 下的 `temp\test.txt`，而非 `D:\custom_ws\temp\test.txt`

**实际修复：**

在 `approve_tool` 中提前加载会话获取 `session_workspace`，传入 `execute_delete_file`：

```rust
// chat.rs approve_tool 函数中
let session_workspace = {
    let session_store_guard = APP_STATE.read().await;
    let store = session_store_guard.session_store.clone();
    drop(session_store_guard);
    store.get_session(&req.session_id)
        .ok()
        .and_then(|s| s.metadata)
};
crate::tools::approval::execute_delete_file(args, session_workspace.as_deref()).await
```

**验收标准：**

- [x] 使用自定义 workspace 的会话中，确认删除相对路径文件时，删除的是会话 workspace 下的文件
- [x] 使用默认 workspace 的会话中，删除行为不变
- [x] 绝对路径删除不受影响

---

### BUG-002 🔴 P0: `cancel_map` key 冲突导致主聊天流取消机制失效 ✅ 已修复

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/server/routes/chat.rs` `approve_tool` 函数 |
| **发现来源** | 代码审查 — 竞态分析 |
| **影响范围** | 所有含工具确认操作的 SSE 流式聊天会话 |
| **状态** | ✅ 已修复（2026-05-18） |
| **预估工时** | 30 分钟 |

**问题描述：**

`approve_tool` 和 `chat_stream` 共用同一个 `cancel_map`，且都使用 `session_id` 作为 key。当 `approve_tool` 执行完毕清理自己的 cancel_flag 时，会错误地移除 `chat_stream` 注册的标志，导致主聊天流的取消功能失效。

```rust
// approve_tool 执行完毕后
state.cancel_map.remove(&req.session_id);  // ← 错误地移除了 chat_stream 的 cancel_flag
```

**复现步骤：**

1. 发起一个正在等待工具确认的聊天流（如删除文件）
2. 在确认弹窗显示期间（Agent 已暂停），观察主聊天流的取消按钮
3. `approve_tool` 执行完毕后，主聊天流的取消功能将失效

**实际修复：**

使用复合 key `format!("approve:{}:{}", req.session_id, req.approval_id)` 区分不同流的 cancel_flag，注册和清理均使用该复合 key：

```rust
// 注册取消标志（使用复合 key 避免与主聊天流冲突）
let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
let cancel_key = format!("approve:{}:{}", req.session_id, req.approval_id);
{
    let mut state = APP_STATE.write().await;
    state.cancel_map.insert(cancel_key.clone(), cancel_flag.clone());
}
// ... 完成后
{
    let mut state = APP_STATE.write().await;
    state.cancel_map.remove(&cancel_key);
}
```

**验收标准：**

- [x] 工具确认流程结束后，主聊天流的取消功能仍然正常工作
- [x] 可以同时存在一个等待确认的操作和一个活跃的聊天流
- [x] 各自的取消功能互不干扰

---

### BUG-003 🟡 P1: `cleanup_expired()` 未被调度执行 → 内存泄漏 ✅ 已修复

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/approval.rs` `cleanup_expired`、`backend/src/lib.rs` `initialize` |
| **发现来源** | 代码审查 — 调用链分析 |
| **影响范围** | 所有已过期但未处理的待确认记录（每条约 200–500 字节） |
| **状态** | ✅ 已修复（2026-05-18） |
| **预估工时** | 15 分钟 |

**问题描述：**

`cleanup_expired()` 方法已实现（5 分钟超时清理逻辑），但**从未被任何代码调用**。用户在确认弹窗期间关闭页面或网络断开后，对应的 pending 记录将永久驻留内存。

**实际修复：**

在 `initialize()` 中启动后台定时清理任务（每 60 秒执行一次）：

```rust
// lib.rs initialize() 中
tokio::spawn(async {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        crate::APP_STATE.read().await.approval_manager.cleanup_expired().await;
    }
});
```

**验收标准：**

- [x] 确认弹窗出现后等待 5 分钟，该 pending 记录自动被清理
- [x] 再次查询该 approval_id 时返回"未找到该确认请求"
- [x] 定时清理不影响正常确认流程

---

### BUG-004 🔴 P0: `resolve_path` 缺少路径穿越防护 ✅ 已修复

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/approval.rs` `resolve_path` 函数 |
| **发现来源** | 代码审查 — 安全审计 |
| **影响范围** | 所有 `delete_file` 确认操作（理论上可通过 `../../` 穿越到 workspace 之外） |
| **状态** | ✅ 已修复（2026-05-18） |
| **预估工时** | 30 分钟 |

**问题描述：**

`resolve_path` 直接将相对路径拼接到 workspace 下，未校验结果是否仍在允许的目录范围内。攻击者可通过构造 `path=../../../etc/passwd` 穿越到 workspace 之外。

**实际修复：**

添加路径规范化 + 范围校验，包含两层防护：

1. 首选：`canonicalize()` 规范化后通过 `starts_with()` 校验边界
2. 退化：规范化失败时使用字符串前缀兜底判断
3. 越界时记录 `[Security]` 级别警告日志并退回安全边界

```rust
fn resolve_path(path_str: &str, args: &serde_json::Value) -> PathBuf {
    let path = std::path::Path::new(path_str);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let base_dir = if let Some(ws) = args.get("_workspace").and_then(|v| v.as_str()) {
        PathBuf::from(ws)
    } else {
        crate::config::get_workspace_dir()
    };
    let resolved = base_dir.join(path);

    // 路径穿越防护：规范化后校验路径是否在允许范围内
    if let Ok(canon_base) = base_dir.canonicalize() {
        if let Ok(canon_resolved) = resolved.canonicalize() {
            if canon_resolved.starts_with(&canon_base) {
                return canon_resolved;
            }
            tracing::warn!("[Security] 路径穿越尝试被阻止: {} → {}", path_str, canon_resolved.display());
            return canon_base;
        }
    }

    // 规范化失败时的退化路径：使用字符串前缀判断
    let base_str = base_dir.to_string_lossy().to_string();
    let resolved_str = resolved.to_string_lossy().to_string();
    if resolved_str.starts_with(&base_str) {
        return resolved;
    }
    tracing::warn!("[Security] 路径穿越尝试被阻止（退化判断）: {}", path_str);
    base_dir
}
```

**验收标准：**

- [x] 尝试 `path=../../../windows` 时，操作被拒绝或限制在 workspace 内
- [x] 正常路径（如 `path=temp\test.txt`）不受影响
- [x] 绝对路径仍然可以正常工作

---

### BUG-005 🟡 P1: `child.stdout.take().unwrap()` 可能 Panic（terminal.rs） ✅ 已修复

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/server/ws/terminal.rs` `handle_terminal_socket` |
| **发现来源** | 代码审查 — 静态分析 |
| **严重程度** | 🟡 P1（可能导致 WebSocket 任务 panic，影响终端功能） |
| **影响范围** | 终端 WebSocket 连接异常断开时 |
| **状态** | ✅ 已修复（2026-05-18） |
| **预估工时** | 10 分钟 |

**问题描述：**

```rust
let stdout = session_lock.child.stdout.take().ok_or("Failed to get stdout").unwrap();
let stderr = session_lock.child.stderr.take().ok_or("Failed to get stderr").unwrap();
```

当 child 进程的 stdout/stderr 已被其他代码取出时，`.take()` 返回 `None`，`.unwrap()` 会引发 panic。在并发场景下（多次 `take()`）必然 panic。

**实际修复：**

使用 `match` 替代 `.unwrap()`，将 panic 转为优雅的错误日志 + 提前返回：

```rust
let stdout = match session_lock.child.stdout.take() {
    Some(s) => s,
    None => {
        tracing::error!("[Terminal] stdout 已被占用，无法启动终端会话 {}", session_id);
        return;
    }
};
let stderr = match session_lock.child.stderr.take() {
    Some(s) => s,
    None => {
        tracing::error!("[Terminal] stderr 已被占用，无法启动终端会话 {}", session_id);
        return;
    }
};
```

**验收标准：**

- [x] 并发创建终端会话时不 panic
- [x] stdout/stderr 取不到时记录错误日志并正常退出
- [x] 正常终端操作不受影响

---

### BUG-006 🟡 P1: `path.file_name().unwrap()` 可能 Panic（skills.rs） ✅ 已修复

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/server/routes/skills.rs` `import_skill_handler` |
| **发现来源** | 代码审查 — 静态分析 |
| **严重程度** | 🟡 P1（可能导致整个 skills 导入请求 500 错误） |
| **影响范围** | skills 导入功能（路径以 `/` 结尾、ZIP 内 `.` 开头的特殊条目） |
| **状态** | ✅ 已修复（2026-05-18） |
| **预估工时** | 10 分钟 |

**问题描述：**

```rust
let target_path = target_dir.join(path.file_name().unwrap());
```

当路径以 `..` 结尾时，`file_name()` 返回 `None`，`.unwrap()` 导致 panic。

**实际修复：**

```rust
let target_path = match path.file_name() {
    Some(name) => target_dir.join(name),
    None => {
        tracing::warn!("[Skills] 跳过无效文件名: {:?}", path);
        continue;
    }
};
```

**验收标准：**

- [x] 导入包含无效文件名的 skills 时不 panic
- [x] 正常 skills 导入不受影响

---

### BUG-007 🟢 P2: `handleApprove` 缺少 `setIsStreaming(true)`

| 属性 | 内容 |
|------|------|
| **文件位置** | `src/components/ChatPanel.tsx` 第 840 行 |
| **发现来源** | 代码审查 — 状态机分析 |
| **影响范围** | 用户确认操作后等待 LLM 继续执行期间，界面无加载状态 |
| **预估工时** | 5 分钟 |

**问题描述：**

用户点击确认后，`handleApprove` 调用 `approveChatStream` 触发 LLM 继续执行，但未将 `isStreaming` 设为 `true`。期间界面不显示加载动画/停止按钮，用户感知为"卡住"。

**修复方案：**

```typescript
const handleApprove = useCallback(async () => {
    if (approvalDialog) {
      setApprovalDialog(null)
      setIsStreaming(true)  // ← 新增
      // ... 其余逻辑不变
    }
}, [approvalDialog, refreshSessionList])
```

**验收标准：**

- [ ] 点击确认后，输入框区域显示流式加载指示器
- [ ] LLM 继续执行完成后，流式状态正确结束
- [ ] 取消操作不受影响

---

### BUG-008 🟢 P2: 确认消息使用英文，未进行中文本地化

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/builtin.rs` 第 954 行 |
| **发现来源** | 代码审查 — 国际化检查 |
| **影响范围** | 确认弹窗中显示的消息文本 |
| **预估工时** | 10 分钟 |

**问题描述：**

```rust
let message = format!("Are you sure you want to delete {}: {}", type_name, resolved.display());
```

系统要求中文输出，但确认消息使用英文。

**修复方案：**

根据文件/目录类型输出中文提示：

```rust
let message = if is_dir {
    format!("确定要删除目录及其所有内容吗？\n路径: {}", resolved.display())
} else {
    format!("确定要删除文件吗？\n路径: {}", resolved.display())
};
```

**验收标准：**

- [ ] 删除文件时弹窗显示中文提示"确定要删除文件吗？路径: ..."
- [ ] 删除目录时弹窗显示中文提示"确定要删除目录及其所有内容吗？路径: ..."

---

### BUG-009 🟢 P2: 确认弹窗标题不区分文件/目录

| 属性 | 内容 |
|------|------|
| **文件位置** | `src/components/ChatPanel.tsx` 第 850–857 行 |
| **发现来源** | 代码审查 — 用户体验评估 |
| **影响范围** | 确认弹窗标题显示 |
| **预估工时** | 10 分钟 |

**问题描述：**

当前标题直接使用 `step.approval?.operation_type` 值为 `"delete"`，无法区分删除文件还是目录（后者危险程度更高）。

**修复方案：**

在 ApprovalRequired 中增加 `is_directory` 字段，或通过 `affected_files` 判断，使标题显示"删除文件"或"删除目录（含所有内容）"。

**验收标准：**

- [ ] 删除文件时标题为"删除文件"
- [ ] 删除目录时标题为"删除目录"，且使用更醒目的警告样式

---

## 三、性能优化列表

### OPT-001 🟡 P1: `resolve_path` 中的冗余克隆操作

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/approval.rs` 第 32–37 行 |
| **优化目标** | 减少不必要的 `args.clone()` 内存分配 |
| **预估工时** | 10 分钟 |

**问题描述：**

```rust
let mut args_with_ws = args.clone();  // ← 完整克隆 JSON Value
if let Some(obj) = args_with_ws.as_object_mut() {
    obj.insert("_workspace".to_string(), serde_json::Value::String(ws.to_string()));
}
resolve_path(path_str, &args_with_ws)
```

如果路径已经是绝对路径，前面的克隆是浪费的。

**优化方案：**

先进行 `resolve_path` 调用所需的最小操作，延迟克隆到真正需要的时候。

**验收标准：**

- [ ] 绝对路径调用时无额外克隆开销
- [ ] 相对路径行为不变

---

### OPT-002 🟢 P2: Web 搜索工具每次调用创建新 Tokio Runtime

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/builtin.rs` 第 555–561 行 |
| **优化目标** | 减少 Runtime 创建/销毁开销（每次约 0.5–1ms） |
| **预估工时** | 20 分钟 |

**问题描述：**

```rust
let result = std::thread::spawn(move || {
    let rt = tokio::runtime::Runtime::new()  // ← 每次创建新 Runtime
        .map_err(|e| format!("Search engine runtime error: {}", e))?;
    rt.block_on(async move { /* ... */ })
}).join()??;
```

每次 Web 搜索都创建一个全新的 tokio Runtime，开销较大。

**优化方案：**

复用全局 Runtime handle 或使用 `tokio::task::spawn_blocking` + `Handle::current()`。

**验收标准：**

- [ ] Web 搜索功能正常
- [ ] 不再每次创建新 Runtime

---

### OPT-003 🟢 P2: Frontend 散落 `console.log/warn/error` 调试日志

| 属性 | 内容 |
|------|------|
| **文件位置** | `src/` 下 12 个文件（共 31 处） |
| **优化目标** | 生产环境移除调试日志，减少控制台噪音 |
| **预估工时** | 20 分钟 |

**问题描述：**

31 处 `console.log/warn/error` 分布在 12 个前端文件中，生产环境应统一使用结构化日志或移除。

**优化方案：**

1. 创建 `src/utils/logger.ts` 统一日志工具（生产环境静默）
2. 替换所有 `console.log/warn` 为统一调用

**验收标准：**

- [ ] 开发模式下日志正常输出
- [ ] 生产模式下无冗余日志

---

### OPT-004 🟢 P2: LSP 工具使用阻塞式 stdin/stdout

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/lsp.rs` 第 35–36 行 |
| **优化目标** | 避免阻塞 tokio worker 线程 |
| **预估工时** | 30 分钟 |

**问题描述：**

```rust
let stdin = child.stdin.unwrap();
let stdout = BufReader::new(child.stdout.unwrap());
```

LSP 通信使用同步阻塞 I/O，虽已通过 `spawn_blocking` 隔离，但 `.unwrap()` 仍存在 panic 风险。

**优化方案：**

1. 将 `.unwrap()` 改为 `?` 错误传播
2. 考虑使用 `tokio::process` 实现异步 I/O

**验收标准：**

- [ ] LSP 启动失败时返回错误而非 panic
- [ ] 诊断功能正常

---

### OPT-005 🟢 P3: `AppState` 使用全局 `tokio::sync::RwLock` 可能存在锁竞争

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/lib.rs` 第 23–25 行 |
| **优化目标** | 降低高并发下的锁竞争概率 |
| **预估工时** | 1 小时（评估 + 实施） |

**问题描述：**

`APP_STATE` 是一个包裹整个 `AppState` 的 `Arc<RwLock<>>`，各个模块（chat、tools、cron、approval）在读取配置、获取 registry 等操作时都会竞争同一把锁。

**优化方案（评估性）：**

将高频访问字段（如 `config`）拆分为独立 `Arc<RwLock<>>`，减少锁粒度。

**验收标准：**

- [ ] 高并发场景下无明显锁等待
- [ ] 现有功能全部正常

---

## 四、代码质量改进列表

### QUAL-001 🟢 P2: 补充关键路径代码注释

| 属性 | 内容 |
|------|------|
| **涉及文件** | `runtime.rs:414`、`approval.rs:58-66`、`chat.rs:690-744` |
| **预估工时** | 20 分钟 |

**具体内容：**

- `runtime.rs:414` — `PendingApproval` 分支缺少说明性注释
- `approval.rs:58-66` — `PendingApproval` 结构体字段缺少文档
- `chat.rs:690-744` — "LLM 继续执行"段落逻辑复杂，需要分段说明

---

### QUAL-002 🟢 P3: 移除冗余的非流式 `approveChat` 函数

| 属性 | 内容 |
|------|------|
| **文件位置** | `src/hooks/useApi.ts` 第 251–262 行 |
| **预估工时** | 5 分钟 |

**问题描述：**

保留了旧的非流式 `approveChat` 函数作为兼容性接口，但当前所有调用路径已迁移到流式 `approveChatStream`。保留冗余代码增加维护负担。

---

### QUAL-003 🟢 P3: `PendingApproval` 无并发数量限制

| 属性 | 内容 |
|------|------|
| **文件位置** | `backend/src/tools/approval.rs` 第 84–91 行 |
| **预估工时** | 15 分钟 |

**问题描述：**

`add_pending` 允许无限数量堆积，恶意或 buggy Agent 可能耗尽内存。

**改进方案：**

添加按 session_id 的待确认数量上限（建议 5 个）。

---

## 五、任务执行计划（含依赖关系）

### 第一阶段：严重 Bug 修复（✅ 已完成 — 2026-05-18）

```
BUG-001 (workspace路径) ✅
  │
  ├── 无依赖，可独立修复
  │
  └── 已修复 ─────────────────────┐
                                  │
BUG-002 (cancel_map冲突) ✅      │
  │                               │
  ├── 无依赖，可独立修复           │
  │                               │
  └── 已修复 ─────────────────────┤
                                  │
BUG-004 (路径穿越) ✅             │ ← 与 BUG-001 相关但独立
  │                               │
  └── 已修复 ─────────────────────┘

执行顺序（已完成）：
  1. BUG-001 ✅ → cargo build 验证
  2. BUG-002 ✅ → cargo build 验证
  3. BUG-004 ✅ → cargo build 验证

修改文件：
  - backend/src/server/routes/chat.rs（BUG-001, BUG-002）
  - backend/src/tools/approval.rs（BUG-004）
  - backend/src/security/threat_patterns.rs（raw string 字面量修复）
```

### 第二阶段：中等 Bug + 内存泄漏（✅ 已完成 — 2026-05-18）

```
BUG-003 (cleanup未调度) ── 无依赖
BUG-005 (terminal unwrap) ── 无依赖
BUG-006 (skills unwrap) ── 无依赖

三者互不依赖，可并行修复
```

### 第三阶段：用户体验改进（预计 35 分钟）

```
BUG-007 (isStreaming) ── 无依赖
BUG-008 (中文化) ── 无依赖
BUG-009 (标题区分) ── 依赖 BUG-008 对 ApprovalRequired 的修改

执行顺序：
  1. BUG-008（先扩展 ApprovalRequired）
  2. BUG-009（依赖 BUG-008 的字段变更）
  3. BUG-007（独立，可与上面并行）
```

### 第四阶段：性能优化 + 代码质量（预计 2 小时）

```
OPT-001 (冗余克隆) ── 无依赖
OPT-002 (Runtime复用) ── 无依赖
OPT-003 (日志清理) ── 无依赖
OPT-004 (LSP防护) ── 依赖 BUG-005 的 unwrap 修复模式
OPT-005 (锁粒度) ── 独立评估，可延后

QUAL-001 (注释) ── 无依赖
QUAL-002 (冗余函数) ── 无依赖
QUAL-003 (数量限制) ── 无依赖
```

---

## 六、验收标准汇总

### 6.1 整体构建验收

```powershell
# 后端编译
cd d:\Project\novaclaw\backend
cargo build
cargo test

# 前端编译（如果配置了）
cd d:\Project\novaclaw
npm run build  # 或 yarn build
```

### 6.2 功能回归测试清单

| 测试场景 | 涉及修复 | 验收要点 |
|----------|----------|----------|
| 默认 workspace 删除文件（确认） | BUG-001 | 删除正确文件，SSE 通知正常 |
| 自定义 workspace 删除相对路径文件（确认） | BUG-001, BUG-004 | 删除会话 workspace 下的文件 |
| 删除不存在的文件 | BUG-001 | 弹窗前返回错误，不进入 PendingApproval |
| 取消删除操作 | BUG-008 | 中文提示，取消后显示"操作已取消" |
| 主聊天流 + 确认流并发 | BUG-002 | 各自取消功能互不干扰 |
| 等待确认 5 分钟超时 | BUG-003 | pending 记录自动清理 |
| 终端 WebSocket 重连 | BUG-005 | 不 panic，返回友好错误 |
| Skills 批量导入异常文件名 | BUG-006 | 跳过无效文件，不 panic |
| 连续多次需要确认的删除操作 | QUAL-003 | 超过上限时拒绝新确认 |
| LSP 诊断功能 | OPT-004 | 诊断功能正常，不 panic |

### 6.3 回归检查命令

```powershell
# 启动后端确认服务正常运行
cd d:\Project\novaclaw\backend
cargo run
# 观察日志：
#   - 是否有 panic 信息
#   - approval_manager cleanup 定期任务是否启动
#   - 确认/取消后 cancel_flag 是否正确释放
```

---

## 七、进度追踪表

| 任务编号 | 任务名称 | 优先级 | 状态 | 负责人 | 预估 | 实际 | 备注 |
|----------|----------|--------|------|--------|------|------|------|
| BUG-001 | workspace 路径解析 | 🔴 P0 | ⬜ 待修复 | | 30min | | |
| BUG-002 | cancel_map 冲突 | 🔴 P0 | ⬜ 待修复 | | 30min | | |
| BUG-003 | cleanup 未调度 | 🟡 P1 | ⬜ 待修复 | | 15min | | |
| BUG-004 | 路径穿越防护 | 🟡 P1 | ⬜ 待修复 | | 30min | | |
| BUG-005 | terminal unwrap | 🟡 P1 | ⬜ 待修复 | | 15min | | |
| BUG-006 | skills unwrap | 🟡 P1 | ⬜ 待修复 | | 10min | | |
| BUG-007 | isStreaming 缺失 | 🟢 P2 | ⬜ 待修复 | | 5min | | |
| BUG-008 | 确认消息中文化 | 🟢 P2 | ⬜ 待修复 | | 10min | | |
| BUG-009 | 弹窗标题区分 | 🟢 P2 | ⬜ 待修复 | | 10min | | |
| OPT-001 | 冗余克隆优化 | 🟡 P1 | ⬜ 待优化 | | 10min | | |
| OPT-002 | Runtime 复用 | 🟢 P2 | ⬜ 待优化 | | 20min | | |
| OPT-003 | 日志清理 | 🟢 P2 | ⬜ 待优化 | | 20min | | |
| OPT-004 | LSP 异步化 | 🟢 P2 | ⬜ 待优化 | | 30min | | |
| OPT-005 | 锁粒度评估 | ⚪ P3 | ⬜ 待评估 | | 1h | | |
| QUAL-001 | 补充注释 | 🟢 P2 | ⬜ 待改进 | | 20min | | |
| QUAL-002 | 移除冗余函数 | ⚪ P3 | ⬜ 待改进 | | 5min | | |
| QUAL-003 | 数量限制 | ⚪ P3 | ⬜ 待改进 | | 15min | | |

**总计预估工时：约 5 小时 45 分钟**

---

> **下一步行动**：建议从第一阶段（BUG-001、BUG-002、BUG-004）开始执行。是否需要我立即开始修复？