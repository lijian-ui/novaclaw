# Token 用量与缓存命中率渲染问题修复文档

## 问题描述

前端 ChatPanel 中 Token 统计（本次输入/输出、累计输入/输出、缓存）有时能渲染，有时不能。缓存命中率（`cache_hit_rate`）同样问题，且表现更严重——大部分场景不显示。

## 根因分析

### 数据链路

```
后端 JSONL → API 返回 → ChatContext → ChatPanel 转换 → ChatMessages 渲染
    ✅            ✅            ✅           ❌ 漏了分支          ✅
```

数据在存储、API 传输、Context 保存阶段都是正确的，问题出在 `ChatPanel.tsx` 中 `contextMessages → MessageData[]` 的转换逻辑。

### 问题代码位置

**文件**：`src/components/ChatPanel.tsx`

该文件中存在一个 `useEffect`，负责将 ChatContext 中的 `contextMessages` 转换为 `MessageData[]` 供 `ChatMessages` 渲染。转换逻辑按消息类型分了 **4 个分支**：

| 分支 | 触发条件 | 代码行号（约） | Token 字段 | cacheHitRate |
|------|----------|---------------|-----------|-------------|
| 1 | 有 `tool_calls` 的 assistant 消息 | L175-L268 | ❌ 全部缺失 | ❌ 缺失 |
| 2 | 有 `first_reasoning`/`again_reasonings` 的 assistant 消息 | L270-L380 | ✅ 有 | ❌ 缺失 |
| 3 | `role=tool` 的工具结果消息 | L382-L395 | N/A（agent_step） | N/A |
| 4 | 普通消息（兜底） | L397-L418 | ✅ 有 | ✅ 有 |

**根本原因**：Token 字段映射（`inputTokens`、`outputTokens`、`cachedTokens`、`lastInputTokens`、`lastOutputTokens`、`cacheHitRate`）在 3 处重复编写，但分支 1 和分支 2 维护不完整。

这就是"有时能渲染，有时不能"的原因——消息类型不同，走的转换分支不同：
- **普通对话**（无工具调用、无思考过程）→ 走分支 4 → 能正常显示
- **涉及工具调用的对话** → 走分支 1 → Token 和缓存命中率全部不显示
- **涉及思考过程的对话** → 走分支 2 → Token 能显示但缓存命中率不显示

## 修复方案

### 原则

**消息结构展开与字段传递分离**。Token 统计和 `cache_hit_rate` 是挂在 final assistant 消息上的数据，和展示逻辑无关，不应该因为走了不同展示分支就丢掉这些字段。

### 具体修改

#### 修改 1：分支 1（tool_calls）添加字段

**文件**：`src/components/ChatPanel.tsx`
**位置**：约 L258-L268，分支 1 的 `// 4️⃣ 最后添加 assistant 消息本身` 处

**修改前**：
```typescript
// 4️⃣ 最后添加 assistant 消息本身（最终回复）
if (m.content && m.content.trim()) {
    converted.push({
        id: m.id,
        role,
        content: m.content,
    })
}
```

**修改后**：
```typescript
// 4️⃣ 最后添加 assistant 消息本身（最终回复）
if (m.content && m.content.trim()) {
    converted.push({
        id: m.id,
        role,
        content: m.content,
        inputTokens: (m as any).inputTokens ?? (m as any).input_tokens,
        outputTokens: (m as any).outputTokens ?? (m as any).output_tokens,
        cachedTokens: (m as any).cachedTokens ?? (m as any).cached_tokens,
        lastInputTokens: (m as any).lastInputTokens ?? (m as any).last_input_tokens,
        lastOutputTokens: (m as any).lastOutputTokens ?? (m as any).last_output_tokens,
        cacheHitRate: (m as any).cacheHitRate ?? (m as any).cache_hit_rate,
    })
}
```

#### 修改 2：分支 2（reasoning）添加 cacheHitRate 字段

**文件**：`src/components/ChatPanel.tsx`
**位置**：约 L369-L379，分支 2 的 `// 添加 assistant 消息本身` 处

**修改前**：
```typescript
if (strippedContent) {
    converted.push({
        id: m.id,
        role: 'assistant',
        content: strippedContent,
        inputTokens: (m as any).inputTokens ?? (m as any).input_tokens,
        outputTokens: (m as any).outputTokens ?? (m as any).output_tokens,
        cachedTokens: (m as any).cachedTokens ?? (m as any).cached_tokens,
        lastInputTokens: (m as any).lastInputTokens ?? (m as any).last_input_tokens,
        lastOutputTokens: (m as any).lastOutputTokens ?? (m as any).last_output_tokens,
    })
}
```

**修改后**：
```typescript
if (strippedContent) {
    converted.push({
        id: m.id,
        role: 'assistant',
        content: strippedContent,
        inputTokens: (m as any).inputTokens ?? (m as any).input_tokens,
        outputTokens: (m as any).outputTokens ?? (m as any).output_tokens,
        cachedTokens: (m as any).cachedTokens ?? (m as any).cached_tokens,
        lastInputTokens: (m as any).lastInputTokens ?? (m as any).last_input_tokens,
        lastOutputTokens: (m as any).lastOutputTokens ?? (m as any).last_output_tokens,
        cacheHitRate: (m as any).cacheHitRate ?? (m as any).cache_hit_rate,
    })
}
```

### 涉及的相关文件

| 文件 | 作用 | 是否需要修改 |
|------|------|------------|
| `backend/src/storage.rs` | 后端 Message 结构体，已包含 `cache_hit_rate: Option<f64>` 字段 | ✅ 无需修改 |
| `backend/src/server/routes/sessions.rs` | 获取消息 API，直接返回 `session_store.get_messages()` 结果 | ✅ 无需修改 |
| `backend/src/server/routes/chat.rs` | SSE done 事件中已返回 `cache_hit_rate` 和 `cache_hit_tokens` | ✅ 无需修改 |
| `src/types/index.ts` | `Message` 接口已包含 `cache_hit_rate?: number` 字段 | ✅ 无需修改 |
| `src/hooks/useApi.ts` | `SseCallbacks.onDone` 已包含 `cache_hit_rate` 和 `cache_hit_tokens` | ✅ 无需修改 |
| `src/components/ChatPanel.tsx` | contextMessages → MessageData 转换，3 个分支重复字段映射 | **❌ 需修改（已修复）** |
| `src/components/ChatMessages.tsx` | 渲染 Token 统计和缓存命中率，一行代码处理所有情况 | ✅ 无需修改 |
| `src/components/Sidebar.tsx` | 加载历史消息时已正确映射 `cache_hit_rate` | ✅ 无需修改 |

## 后续重构建议

当前 4 个分支各自重复编写字段映射，维护成本高、容易遗漏。更好的做法是**将字段映射集中到一处**：

```typescript
// 提取统一的消息构建函数
function buildAssistantMessage(m: any) {
    const strippedContent = m.content
        .replace(/<think\s*>[\s\S]*?<\/think\s*>/gi, '')
        .replace(/<think\s*>[\s\S]*$/i, '')
        .replace(/<\|channel\|?>thought[\s\S]*?<channel\|>/gi, '')
        .trim()
    return {
        id: m.id,
        role: 'assistant',
        content: strippedContent || m.content,
        // Token 字段集中在此处，所有分支共用
        inputTokens: (m as any).inputTokens ?? (m as any).input_tokens,
        outputTokens: (m as any).outputTokens ?? (m as any).output_tokens,
        cachedTokens: (m as any).cachedTokens ?? (m as any).cached_tokens,
        lastInputTokens: (m as any).lastInputTokens ?? (m as any).last_input_tokens,
        lastOutputTokens: (m as any).lastOutputTokens ?? (m as any).last_output_tokens,
        cacheHitRate: (m as any).cacheHitRate ?? (m as any).cache_hit_rate,
    }
}
```

然后 4 个分支只关心展开逻辑（tool_calls 展开、reasoning 展开），添加 assistant 消息时统一调用 `buildAssistantMessage(m)`。这样新增字段只需改一处。