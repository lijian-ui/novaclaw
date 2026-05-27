# DeepSeek-Reasonix 深度技术分析报告

> 分析日期：2026-05-26 | 目标：`d:\Project\novaclaw\参考项目\DeepSeek-Reasonix` | 版本：v0.48.0

---

## 一、项目概述

**DeepSeek-Reasonix** 是一个专为 DeepSeek 模型设计的 AI 编程 Agent CLI/TUI 工具，采用 **Cache-First** 架构设计，实测缓存命中率可达 **99.82%**。项目核心定位是"一个便宜到可以一直开着的编程 Agent"——通过对 DeepSeek 前缀缓存机制的深度适配，将缓存未命中时的成本降低约 90%。

### 核心指标

| 指标 | 数据 |
|------|------|
| 日处理输入 Token | 4.35 亿 (2026-05-01 真实用户数据) |
| 缓存命中率 | 99.82% |
| 缓存节省成本 | ~80%（v4-flash），~91%（v4-pro） |
| 日实际花费 | ~$12.34 vs 无缓存 ~$60.63 |
| 上下文窗口 | 1,000,000 tokens |

### 技术栈

- **语言/运行时**：TypeScript 5.6+, ES2022, ESM, Node ≥22
- **CLI 框架**：Commander.js + Ink 5 (React 18) TUI
- **测试**：Vitest 2.x
- **构建**：tsup (打包), tsx (开发运行)
- **MCP 协议**：stdio + SSE + Streamable HTTP
- **无外部 AI 框架依赖**——直接调用 DeepSeek REST API

---

## 二、项目结构

```
DeepSeek-Reasonix/
├── src/                                    # 核心源码
│   ├── client.ts                           # DeepSeek HTTP/SSE 客户端
│   ├── loop.ts                             # CacheFirstLoop 主循环（核心）
│   ├── context-manager.ts                  # 上下文管理（折叠/压缩策略）
│   ├── tokenizer.ts                        # DeepSeek V4 分词器（完整移植）
│   ├── types.ts                            # 核心类型定义
│   ├── config.ts                           # 配置管理（Zod 验证）
│   ├── prompt-fragments.ts                 # 共享提示词片段
│   ├── hooks.ts                            # 生命周期钩子系统
│   ├── index.ts                            # 库入口
│   ├── env.ts                              # 环境变量
│   │
│   ├── repair/                             # 工具调用修复流水线
│   │   ├── index.ts                        # 修复编排（4道工序）
│   │   ├── scavenge.ts                     # 从 reasoning_content 中打捞工具调用
│   │   ├── flatten.ts                      # 参数扁平化（深嵌套→点记法）
│   │   ├── truncation.ts                   # 截断JSON修复
│   │   └── storm.ts                        # 风暴抑制（重复调用检测）
│   │
│   ├── memory/                             # 内存系统
│   │   ├── runtime.ts                      # ImmutablePrefix + AppendOnlyLog + VolatileScratch
│   │   ├── session.ts                      # JSONL 会话持久化
│   │   ├── project.ts                      # 项目记忆 (REASONIX.md)
│   │   ├── user.ts                         # 用户记忆 (~/.reasonix/memory/)
│   │   └── subdir.ts                       # 子目录记忆
│   │
│   ├── tools/                              # 工具实现
│   │   ├── filesystem.ts                   # 文件系统工具 (read/write/edit/search)
│   │   ├── shell.ts                        # Shell 命令执行
│   │   ├── memory.ts                       # 记忆管理工具
│   │   ├── skills.ts                       # 技能系统
│   │   ├── subagent.ts                     # 子Agent
│   │   ├── plan.ts                         # 规划系统
│   │   ├── web.ts                          # 网络搜索
│   │   └── ...
│   │
│   ├── cli/                                # CLI 入口 + TUI
│   │   ├── index.ts                        # Commander 入口
│   │   ├── commands/                       # 命令实现 (chat, code, run, ...)
│   │   └── ui/                             # Ink TUI 组件
│   │       ├── App.tsx                     # 根组件
│   │       ├── LiveRows.tsx                # 实时状态行
│   │       ├── StatsPanel.tsx              # 统计面板（成本/缓存命中率）
│   │       └── ...
│   │
│   ├── mcp/                                # MCP 客户端
│   │   ├── client.ts                       # MCP 客户端
│   │   ├── stdio.ts                        # stdio 传输
│   │   ├── sse.ts                          # SSE 传输
│   │   └── ...
│   │
│   ├── code/                               # SEARCH/REPLACE 编辑系统
│   │   ├── edit-blocks.ts                  # 编辑块解析器
│   │   ├── diff-preview.ts                 # Diff 预览
│   │   └── lifecycle.ts                    # 编辑生命周期
│   │
│   ├── telemetry/                          # 遥测/统计
│   │   ├── stats.ts                        # 会话统计 + 成本计算
│   │   └── usage.ts                        # 使用量上报
│   │
│   ├── loop/                               # 循环子系统
│   │   ├── thinking.ts                     # 思考模式检测
│   │   ├── messages.ts                     # 消息构建
│   │   ├── errors.ts                       # 错误处理
│   │   ├── escalation.ts                   # 自动升级
│   │   ├── healing.ts                      # 消息修复
│   │   ├── shrink.ts                       # 工具结果压缩
│   │   └── force-summary.ts               # 强制摘要
│   │
│   └── server/                             # Dashboard HTTP 服务器
│
├── packages/
│   └── core-utils/                         # 核心工具包
│
├── data/
│   └── deepseek-tokenizer.json.gz          # DeepSeek V4 分词器数据
│
├── benchmarks/                             # 基准测试
│   ├── real-world-cache/                   # 真实用户缓存命中率案例
│   └── tau-bench/                          # τ-bench 测试
│
├── scripts/
│   ├── probe-cache.mjs                     # 缓存命中率探测脚本
│   ├── probe-loop-cache.mts                # 循环缓存探测
│   └── probe-lifecycle-cache-neutral.mts   # 生命周期缓存中性探测
│
├── docs/
│   ├── ARCHITECTURE.md                     # 架构文档
│   └── ...
│
└── package.json                            # 项目配置
```

---

## 三、架构核心：Cache-First Loop

该项目最核心的架构决策是将 DeepSeek 的**前缀缓存（Prefix Caching）**机制作为整个系统的设计基础。与通用 Agent 框架不同，Reasonix 的一切设计都围绕"保持前缀字节稳定"这一目标展开。

### 3.1 三层上下文分区

[src/memory/runtime.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/memory/runtime.ts)

```
┌─────────────────────────────────────────┐
│ IMMUTABLE PREFIX                        │ ← 会话期间固定
│   system + tool_specs + few_shots        │   缓存命中候选
├─────────────────────────────────────────┤
│ APPEND-ONLY LOG                         │ ← 单调增长
│   [assistant₁][tool₁][assistant₂]...    │   保持先前回合的前缀
├─────────────────────────────────────────┤
│ VOLATILE SCRATCH                        │ ← 每回合重置
│   R1 思考, 临时规划状态                 │   永不发送给 API
└─────────────────────────────────────────┘
```

**三个不变约束：**
1. **Prefix 会话内只计算一次**，hash 固定，后续请求不会因拼写差异导致缓存未命中
2. **日志只追加（Append-Only）**，不允许原位重写，确保历史字节序列稳定
3. **Scratch 区域永远不进入 API 请求**，思考内容不污染前缀

### 3.2 ImmutablePrefix 实现

[src/memory/runtime.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/memory/runtime.ts#L10-L76)

```typescript
/**
 * 核心：前缀指纹（fingerprint）机制
 * 任何 addTool/removeTool/replaceSystem 都会使指纹失效
 */
export class ImmutablePrefix {
  system: string;
  private _toolSpecs: ToolSpec[];
  readonly fewShots: readonly ChatMessage[];
  private _fingerprintCache: string | null = null;

  /**
   * 指纹通过 SHA256(system + tools + fewShots) 生成
   */
  private computeFingerprint(): string {
    const blob = JSON.stringify({
      system: this.system,
      tools: this._toolSpecs,
      shots: this.fewShots,
    });
    return createHash("sha256").update(blob).digest("hex").slice(0, 16);
  }

  /**
   * addTool 会使指纹缓存失效，下一次请求必定缓存未命中
   */
  addTool(spec: ToolSpec): boolean {
    const name = spec.function?.name;
    if (!name) return false;
    if (this._toolSpecs.some((t) => t.function?.name === name)) return false;
    this._toolSpecs.push(spec);
    this._fingerprintCache = null;  // ← 关键：无效化缓存
    return true;
  }

  /**
   * verifyFingerprint() 在 dev/test 模式下检测缓存漂移
   */
  verifyFingerprint(): string {
    const fresh = this.computeFingerprint();
    if (this._fingerprintCache !== null && this._fingerprintCache !== fresh) {
      throw new Error(
        `ImmutablePrefix fingerprint drift: cached=${this._fingerprintCache}, fresh=${fresh}.`
      );
    }
    this._fingerprintCache = fresh;
    return fresh;
  }
}
```

### 3.3 AppendOnlyLog 实现

[src/memory/runtime.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/memory/runtime.ts#L79-L119)

```typescript
/**
 * 严格只追加的日志系统
 * 唯一允许原位重写的路径是 /compact 命令和会话恢复时的修复
 */
export class AppendOnlyLog {
  private _entries: ChatMessage[] = [];

  append(message: ChatMessage): void {
    if (!message || typeof message !== "object" || !("role" in message)) {
      throw new Error(`invalid log entry: ${JSON.stringify(message)}`);
    }
    this._entries.push(message);  // ← 只追加，不修改历史
  }

  /** 唯一例外：/compact 和 recovery 路径 */
  compactInPlace(replacement: ChatMessage[]): void {
    this._entries = [...replacement];
  }

  get entries(): readonly ChatMessage[] {
    return this._entries;
  }
}
```

---

## 四、缓存机制深度分析

### 4.1 DeepSeek 前缀缓存工作原理

DeepSeek API 自动对请求的前缀部分进行缓存。当请求的**精确字节前缀**与之前某个请求匹配时，命中的 Token 按 5%-10% 的价格计费。这意味着：
- 如果每次请求的 token 序列完全相同，缓存命中率可达 100%
- 任何字节级别的差异（空格变化、JSON key 顺序变化、时间戳插入）都会导致缓存未命中
- 传统 Agent 框架通常会重新序列化工具列表、插入时间戳、重排历史，导致缓存命中率低于 20%

### 4.2 Reasonix 保持缓存命中的四重机制

#### 机制一：ImmutablePrefix（前缀冻结）

[src/memory/runtime.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/memory/runtime.ts#L10-L76)

- System prompt、tool specs、few-shot 示例在会话开始时冻结
- 每次请求发送完全相同的前缀字节序列
- `addTool()` 会破坏指纹，但只在添加 MCP 工具时触发——Session 内默认工具集不变

#### 机制二：AppendOnlyLog（只追加日志）

[src/memory/runtime.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/memory/runtime.ts#L79-L119)

- 历史消息只追加在末尾，不重排、不修改
- 设计团队通过 [scripts/probe-cache.mjs](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/scripts/probe-cache.mjs) 验证了：**原位修改历史消息会破坏后续所有消息的缓存**，而追加方式几乎不损失缓存命中率

```javascript
// probe-cache.mjs 验证脚本核心逻辑
// Phase 3：原位修改 A 消息然后追加相同的新一轮
// 预期：缓存从修改点开始全部丢失
// 实际：缓存损失高达数万 token，假设被证实
if (lostHit > 100) {
    console.log("VERDICT: in-place mutation destroys cache. Hypothesis confirmed.");
}
```

#### 机制三：VolatileScratch（易失性暂存区）

[src/memory/runtime.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/memory/runtime.ts#L0-L133)

- DeepSeek R1 的思考过程（reasoning_content）、临时规划状态等视为"易失性内容"
- 这些内容**永不发送给 API**，只用于本地展示和决策
- 避免了思考内容进入上下文破坏前缀稳定性

#### 机制四：智能上下文折叠（Auto-Compact）

[src/context-manager.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/context-manager.ts#L1-L250)

当上下文使用率超过阈值时，将早期回合压缩为摘要消息。**压缩后的摘要追加在日志中，不修改原前缀结构**，因此缓存仍然命中早于摘要前的部分。

```typescript
/**
 * 上下文折叠决策逻辑
 * 5个阈值级别，从保守到激进：
 */
export const HISTORY_FOLD_THRESHOLD = 0.5;           // 50% → 开始折叠
export const HISTORY_FOLD_TAIL_FRACTION = 0.2;        // 折叠后保留 20% 尾部
export const HISTORY_FOLD_AGGRESSIVE_THRESHOLD = 0.7; // 70% → 激进折叠
export const HISTORY_FOLD_AGGRESSIVE_TAIL_FRACTION = 0.1; // 激进时只留 10%
export const FORCE_SUMMARY_THRESHOLD = 0.8;           // 80% → 强制退出+摘要
export const PREFLIGHT_EMERGENCY_THRESHOLD = 0.95;    // 95% → 紧急截断

/**
 * 在发送请求前进行预检，同时检查 token 和 body 字节数
 * 因为 DeepSeek 网关对请求体大小有硬性限制（~880KB）
 */
export const MAX_BODY_BYTES = 700_000;   // 网关限制的安全阈值
export const MAX_BODY_BYTES_TARGET = 500_000; // 截断目标
```

### 4.3 缓存命中率度量

[src/client.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/client.ts#L1-L37)

```typescript
export class Usage {
  constructor(
    public promptTokens = 0,
    public completionTokens = 0,
    public totalTokens = 0,
    public promptCacheHitTokens = 0,   // 缓存命中的 token 数
    public promptCacheMissTokens = 0,  // 缓存未命中的 token 数
  ) {}

  /** 缓存命中率 = 命中 / (命中 + 未命中) */
  get cacheHitRatio(): number {
    const denom = this.promptCacheHitTokens + this.promptCacheMissTokens;
    return denom > 0 ? this.promptCacheHitTokens / denom : 0;
  }

  /** 从 API 响应解析缓存统计 */
  static fromApi(raw: RawUsage | undefined | null): Usage {
    const u = raw ?? {};
    const promptTokens = u.prompt_tokens ?? 0;
    const cacheHitTokens = u.prompt_cache_hit_tokens ?? 0;
    const cacheMissTokens =
      u.prompt_cache_miss_tokens ?? Math.max(0, promptTokens - cacheHitTokens);
    return new Usage(
      promptTokens, u.completion_tokens ?? 0, u.total_tokens ?? 0,
      cacheHitTokens, cacheMissTokens,
    );
  }
}
```

### 4.4 缓存命中率真实数据

[benchmarks/real-world-cache/README.md](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/benchmarks/real-world-cache/README.md)

| 指标 | 数值 |
|------|------|
| 缓存命中 Token | 435,033,856 |
| 缓存未命中 Token | 767,616 |
| **缓存命中率** | **99.82%** |
| 日实际费用（v4-flash） | ~$12.34 |
| 无缓存估算费用 | ~$60.63 |
| 节省比例 | ~80% |

对比其他客户端在同一 DeepSeek API 下的表现：
- DeepSeek 官方 Web Chat：60-80%，新会话降至 0%
- Cherry Studio / Open WebUI：30-60%
- Cline / Continue：更低（XML 工具调用内联工具结果，改变字节偏移）

---

## 五、DeepSeek 模型专有优化

### 5.1 V4 聊天模板对齐

[src/tokenizer.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/tokenizer.ts#L449-L530)

完整移植了 DeepSeek V4 的 Python 聊天模板（`encoding_dsv4.py`），包括：
- DSML 工具调用格式（`<tool_calls>` / `<invoke>` / `<parameter>`）
- 工具结果合并到 User 消息的机制
- thinking/reasoning_content 的模板处理
- 上行消息中删除多余的 reasoning_content（仅保留最后一个 User 之后的）

```typescript
/** 应用 DeepSeek V4 聊天模板 */
export function formatDeepSeekPrompt(
  messages: Array<{ role?: string; content?: string | null; tool_calls?: unknown; ... }>,
  drop_thinking = false,
): string {
  let msgs = messages as V4Message[];
  if (drop_thinking) {
    msgs = dropThinkingMessages(msgs);  // 删除多余 reasoning
  }
  const merged = mergeToolMessages(msgs);  // 工具结果合并到 User
  let prompt = BOS;
  for (let i = 0; i < merged.length; i++) {
    const msg = merged[i]!;
    const role = msg.role ?? "user";
    // 按角色拼接模板：system, user(USER_SP), assistant(含 thinking)
    // ...
  }
  return prompt;
}
```

### 5.2 V4 分词器移植

[src/tokenizer.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/tokenizer.ts#L1-L449)

完整移植 DeepSeek V4 的 BPE 分词器（基于 GPT-2 的 byte-level BPE），用于精确估算 Token 数量：
- 从 `deepseek-tokenizer.json.gz` 加载词汇表和合并规则
- 实现 split → byte-level encode → BPE encode 完整流水线
- 提供 `estimateRequestTokens()` 用于上下文预检

```typescript
/** 精确估算请求 token 数（含工具定义） */
export function estimateRequestTokens(
  messages: ChatMessage[],
  toolSpecs?: ReadonlyArray<unknown> | null,
  drop_thinking = false,
): number {
  let total = estimateConversationTokens(messages, drop_thinking);
  if (toolSpecs && toolSpecs.length > 0) {
    total += countTokensBounded(renderTools(toolSpecs));
  }
  return total;
}
```

### 5.3 思考模式（Thinking Mode）深度适配

[src/loop/thinking.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/loop/thinking.ts#L1-L25)

```typescript
/** 判断模型是否使用思考模式（V4 全系列均为思考模式） */
export function isThinkingModeModel(model: string): boolean {
  if (model.includes("reasoner")) return true;
  if (model === "deepseek-v4-flash" || model === "deepseek-v4-pro") return true;
  return false;
}

/** 设置 extra_body.thinking.type */
export function thinkingModeForModel(model: string): "enabled" | "disabled" | undefined {
  if (model === "deepseek-chat") return "disabled";
  if (model.includes("reasoner")) return "enabled";
  if (model === "deepseek-v4-flash" || model === "deepseek-v4-pro") return "enabled";
  return undefined;
}

/** 剥离幻觉生成的工具调用标记 */
export function stripHallucinatedToolMarkup(s: string): string {
  let out = s;
  out = out.replace(/<function_calls>[\s\S]*?<\/?function_calls>/g, "");
  out = out.replace(/<\|DSML\|function_calls>[\s\S]*?<\/?\|DSML\|function_calls>/g, "");
  out = out.replace(/<function_calls>[\s\S]*?<\/function_calls>/g, "");
  out = out.replace(/<[\s\S]*$/g, "");  // 处理截断的未闭合标记
  return out.trim();
}
```

### 5.4 消息修复（Healing）

[src/loop/healing.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/loop/healing.ts)

会话恢复时自动修复历史消息中的问题：
- 过大工具结果的 Token 压缩
- 思考模式会话中缺失的 `reasoning_content` 补全
- 截断的工具调用修复

```typescript
// 在 loop.ts 构造函数中调用
const shrunk = healLoadedMessagesByTokens(prior, DEFAULT_MAX_RESULT_TOKENS);
// 思考模式下补全 reasoning_content 以避免 API 400 错误
const stamped = stampMissingReasoningForThinkingMode(shrunk.messages, this.model);
```

### 5.5 Azure OpenAI 端点兼容

[src/client.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/client.ts#L108-L115)

```typescript
/** Azure OpenAI 兼容端点不接受 extra_body.thinking 字段 */
private _isAzureEndpoint(): boolean {
  try {
    const host = new URL(this.baseUrl).hostname;
    return host === "azure.com" || host.endsWith(".azure.com");
  } catch { return false; }
}
```

---

## 六、工具调用修复流水线（Tool-Call Repair）

[src/repair/index.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/repair/index.ts#L1-L129)

DeepSeek 在处理复杂工具调用时存在四个已知问题，Reasonix 通过四道修复工序逐一解决：

### 6.1 四道工序

| 工序 | 解决的问题 | 代码位置 |
|------|-----------|---------|
| **Flatten** | 参数超过 10 个或嵌套深度 > 2 时，DeepSeek 会遗漏参数 | `src/repair/flatten.ts` |
| **Scavenge** | DeepSeek 将工具调用 JSON 放在 reasoning_content 中而非 tool_calls 字段 | `src/repair/scavenge.ts` |
| **Truncation** | 输出被 max_tokens 截断导致 JSON 不完整 | `src/repair/truncation.ts` |
| **Storm** | 相同的 (tool, args) 在滑动窗口内重复调用 | `src/repair/storm.ts` |

### 6.2 修复编排

```typescript
/** ToolCallRepair 是四道工序的编排器 */
export class ToolCallRepair {
  private readonly storm: StormBreaker;

  process(
    declaredCalls: ToolCall[],      // 模型在 tool_calls 字段中声明的调用
    reasoningContent: string | null, // 模型的思考内容
    content: string | null = null,   // 模型的文本回复
  ): { calls: ToolCall[]; report: RepairReport } {
    const report: RepairReport = {
      scavenged: 0, truncationsFixed: 0, stormsBroken: 0, notes: [],
    };

    // 1. Scavenge：从 reasoning_content 中打捞丢失的工具调用
    const combined = [reasoningContent ?? "", content ?? ""].filter(Boolean).join("\n");
    const scavenged = scavengeToolCalls(combined || null, {
      allowedNames: this.opts.allowedToolNames,
      maxCalls: this.opts.maxScavenge ?? 4,
    });
    // 合并到声明的调用中，去重
    for (const sc of scavenged.calls) {
      if (!seenSignatures.has(signature(sc))) {
        merged.push(sc); report.scavenged++;
      }
    }

    // 2. Truncation：修复截断的 JSON 参数
    for (const call of merged) {
      const r = repairTruncatedJson(call.function?.arguments ?? "");
      if (r.changed) { call.function.arguments = r.repaired; report.truncationsFixed++; }
    }

    // 3. Storm：抑制重复调用风暴
    for (const call of merged) {
      const verdict = this.storm.inspect(call);
      if (verdict.suppress) { report.stormsBroken++; continue; }
      filtered.push(call);
    }

    return { calls: filtered, report };
  }
}
```

### 6.3 风暴抑制（Storm Breaker）

[src/repair/storm.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/repair/storm.ts)

```typescript
/**
 * 风暴抑制器
 * 在滑动窗口内跟踪 (tool_name, args_signature) 的出现次数
 * 超过阈值则抑制调用
 */
export class StormBreaker {
  private window: Array<{ name: string; argsSig: string }> = [];

  inspect(call: ToolCall): { suppress: boolean; reason?: string } {
    const name = call.function?.name ?? "";
    const argsSig = this.argsSignature(call.function?.arguments ?? "");
    const key = `${name}::${argsSig}`;

    // 统计窗口内相同调用的次数
    const count = this.window.filter(w => `${w.name}::${w.argsSig}` === key).length + 1;

    if (count >= this.threshold) {
      return { suppress: true, reason: `storm: ${name} called ${count}x` };
    }

    this.window.push({ name, argsSig });
    return { suppress: false };
  }
}
```

---

## 七、DeepSeek 客户端实现

[src/client.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/client.ts#L1-L356)

### 7.1 超时策略

```typescript
/**
 * 11 分钟超时。
 * DeepSeek 的负载均衡器可能保持连接打开长达 10 分钟（请求在队列中等待）
 * 旧版 2 分钟超时会导致排队的请求被过早杀死
 */
this.timeoutMs = opts.timeoutMs ?? 660_000;  // 11 min
```

### 7.2 速率限制

```typescript
private async waitForChatRateLimit(signal?: AbortSignal): Promise<void> {
  if (this.minChatIntervalMs <= 0) return;
  const now = Date.now();
  const waitMs = Math.max(0, this.nextChatRequestAt - now);
  this.nextChatRequestAt = Math.max(now, this.nextChatRequestAt) + this.minChatIntervalMs;
  if (waitMs <= 0) return;
  // 等待速率限制间隔
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(resolve, waitMs);
    // ...
  });
}
```

### 7.3 流式处理

```typescript
async *stream(opts: ChatRequestOptions): AsyncGenerator<StreamChunk> {
  // 初始请求可重试，一旦流开始发送则不重试（避免重复计费）
  resp = await fetchWithRetry(this._fetch, `${this.baseUrl}/chat/completions`, {
    method: "POST",
    headers: { /* ... */ },
    body: JSON.stringify(this.buildPayload(opts, true)),
    signal,
  }, { ...this.retry, signal });

  // 使用 eventsource-parser 解析 SSE 流
  const parser = createParser({
    onEvent: (ev: EventSourceMessage) => {
      // 解析 contentDelta, reasoningDelta, toolCallDelta, usage
      // 检测 DONE 标记
    },
  });
  // ...
}
```

---

## 八、主循环 CacheFirstLoop 完整流程

[src/loop.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/loop.ts#L1-L1249)

### 8.1 构造阶段

```typescript
export class CacheFirstLoop {
  constructor(opts: CacheFirstLoopOptions) {
    this.client = opts.client;
    this.prefix = opts.prefix;
    this.model = opts.model ?? "deepseek-v4-flash";  // 默认 flash
    this.reasoningEffort = opts.reasoningEffort ?? "max";
    this.stream = opts.stream ?? true;

    // 会话恢复时的消息修复
    if (this.sessionName) {
      const prior = loadSessionMessages(this.sessionName);
      const shrunk = healLoadedMessagesByTokens(prior, DEFAULT_MAX_RESULT_TOKENS);
      const stamped = stampMissingReasoningForThinkingMode(shrunk.messages, this.model);
      // 修复后的消息重写回会话文件
    }
  }
}
```

### 8.2 单回合执行流程

```typescript
async *step(userInput: string): AsyncGenerator<LoopEvent> {
  // 1. 预算检查（可选）
  if (this.budgetUsd !== null) {
    if (spent >= this.budgetUsd) { /* 拒绝执行 */ return; }
    if (spent >= this.budgetUsd * 0.8) { /* 发出 80% 警告 */ }
  }

  // 2. 重置回合状态
  this._turn++; this.scratch.reset(); this.repair.resetStorm();

  // 3. 持久化用户消息
  this.appendAndPersist({ role: "user", content: userInput });

  // 4. 内部迭代循环（工具调用 → 模型响应 → 工具调用 → ...）
  for (let iter = 0; ; iter++) {
    // 4a. 检查中止信号
    if (signal.aborted) { /* 生成中止事件 */ return; }

    // 4b. 构建消息（前缀 + 历史 + 当前用户输入）
    let messages = this.buildMessages(pendingUser);

    // 4c. 预检上下文（本地估算，避免 400 错误）
    const decision = this.context.decidePreflight(messages, ...);
    if (decision.needsAction) {
      const result = this.context.mechanicalTruncate(this.model, ...);
      // 发出警告事件
    }

    // 4d. 调用 DeepSeek API（流式或非流式）
    if (this.stream) {
      for await (const chunk of this.client.stream({ ... })) {
        // 处理 reasoningDelta, contentDelta, toolCallDelta
        // 检测 NEEDS_PRO 升级标记
      }
    } else {
      const resp = await this.client.chat({ ... });
    }

    // 4e. 自我报告升级：模型发出 NEEDS_PRO 标记
    if (this.autoEscalate && isEscalationRequest(assistantContent)) {
      this._escalateThisTurn = true;
      // 重置内容，降级 iter，使用 pro 模型重试
      iter--; continue;
    }

    // 4f. 工具调用修复
    const { calls: repairedCalls, report } = this.repair.process(
      toolCalls, reasoningContent, assistantContent
    );

    // 4g. 持久化助手消息
    this.appendAndPersist(buildAssistantMessage(...));

    // 4h. 如果没有工具调用，回合结束
    if (repairedCalls.length === 0) {
      yield { turn: this._turn, role: "done", ... };
      return;
    }

    // 4i. 上下文管理决策
    const decision = this.context.decideAfterUsage(usage, this.model, ...);
    if (decision.kind === "fold") {
      await this.compactHistory({ keepRecentTokens: decision.tailBudget });
    }

    // 4j. 并行执行工具调用
    // 将连续的可并行调用分组，通过 Promise.allSettled 并发执行
    // 并行安全由工具的 parallelSafe 标志控制
    const settled = await Promise.allSettled(chunk.map(c => this.runOneToolCall(c, signal)));

    // 4k. 按声明顺序持久化工具结果
    for (let k = 0; k < chunk.length; k++) {
      this.appendAndPersist({ role: "tool", ... });
    }
  }
}
```

### 8.3 并行工具调度

```typescript
// 分组逻辑：连续的 parallelSafe 调用为一组
const chunk: ToolCall[] = [];
while (
  callIdx < repairedCalls.length &&
  chunk.length < parallelMax &&
  this.tools.isParallelSafe(repairedCalls[callIdx]?.function?.name ?? "")
) {
  chunk.push(repairedCalls[callIdx++]!);
}
// 非并行安全的调用单独执行（串行屏障）
if (chunk.length === 0) {
  chunk.push(repairedCalls[callIdx++]!);
}

// 并发执行，但按声明顺序返回结果
const settled = await Promise.allSettled(chunk.map(c => this.runOneToolCall(c, signal)));
for (let k = 0; k < chunk.length; k++) {
  // 按原始顺序处理结果
}
```

---

## 九、成本控制体系

[src/telemetry/stats.ts](file:///d:/Project/novaclaw/参考项目/DeepSeek-Reasonix/src/telemetry/stats.ts#L1-L224)

### 9.1 定价表

```typescript
/** USD per 1M tokens */
export const DEEPSEEK_PRICING = {
  "deepseek-v4-flash": { inputCacheHit: 0.0028, inputCacheMiss: 0.14, output: 0.28 },
  "deepseek-v4-pro":   { inputCacheHit: 0.003625, inputCacheMiss: 0.435, output: 0.87 },
  // 缓存命中时价格约为未命中的 2%！
};
```

### 9.2 成本计算

```typescript
export function costUsd(model: string, usage: Usage): number {
  const p = pricingFor(model);
  if (!p) return 0;
  return (
    (usage.promptCacheHitTokens * p.inputCacheHit +
      usage.promptCacheMissTokens * p.inputCacheMiss +
      usage.completionTokens * p.output) / 1_000_000
  );
}
```

### 9.3 五层成本控制

| 层级 | 机制 | 触发条件 | 效果 |
|------|------|---------|------|
| 1 | Flash 优先 | 默认 `auto` 预设 | 大多数回合使用 v4-flash（1×成本） |
| 2 | 自动升级 | 模型输出 `<<<NEEDS_PRO>>>` 标记 | 当前回合升级到 v4-pro |
| 3 | 单回合 /pro | 用户输入 `/pro` | 下一回合使用 v4-pro，自动恢复 |
| 4 | 失败触发升级 | 同一回合内 3+ 次工具调用失败 | 当前回合剩余部分使用 v4-pro |
| 5 | 自动上下文压缩 | 上下文使用率 > 50% | 早期回合折叠为摘要，保持缓存命中 |

### 9.4 辅助调用固定使用 Flash

```typescript
// 所有辅助调用（摘要生成、子Agent生成、截断修复重试等）
// 固定使用 v4-flash + effort=high，无论用户预设是什么
// 没有理由为"将这些工具结果改写为散文"支付 pro 级别的费用
```

### 9.5 成本可视化

TUI 底部状态栏实时显示：
- `cache 99.8%` — 缓存命中率
- `turn $0.003` — 当前回合成本（绿色 <$0.05，黄色 $0.05-0.20，红色 ≥$0.20）
- `session $0.12` — 会话累计成本（绿色 <$0.50，黄色 $0.50-2.00，红色 ≥$2.00）

---

## 十、关键设计决策总结

| 决策 | 选择 | 替代方案 | 原因 |
|------|------|---------|------|
| 模型支持 | 仅 DeepSeek | 通用多模型 | 前缀缓存深度绑定，通用框架无法优化 |
| 缓存策略 | 精确前缀匹配 | LRU/LFU/TTL | DeepSeek API 原生支持，零额外开销 |
| 历史管理 | Append-Only | 滑动窗口/摘要替换 | 保持前缀字节稳定 |
| 工具调用格式 | DSML（专用标记语言） | XML/JSON Schema | 与 DeepSeek V4 训练数据对齐 |
| 成本默认 | Flash 优先 | Pro 优先 | 降低使用门槛，自动升级兜底 |
| 并行执行 | 声明式并行安全标记 | 自动检测 | 显式约定比隐式推断更可靠 |
| 会话持久化 | JSONL（每行一条消息） | SQLite/JSON文件 | 简单可靠，支持 append-only |
| 思考模式 | 完整支持，round-trip 保留 | 忽略 | 必须保留 reasoning_content 否则 API 400 |

---

## 十一、性能瓶颈与改进方向

### 11.1 当前瓶颈

| 瓶颈 | 描述 | 影响程度 |
|------|------|---------|
| addTool 破坏缓存 | 每次添加 MCP 工具都改变前缀指纹 | 中（每次 MCP 热插拔触发一次缓存未命中） |
| 初始化缓存预热 | 新会话第一个请求必定缓存未命中 | 低（仅一次） |
| 大文件读取内联 | 读取大文件后会作为工具结果内联到上下文 | 中（后续请求的成本增加） |
| 折叠摘要的额外 API 调用 | 上下文折叠需要额外调用模型生成摘要 | 低（摘要使用 v4-flash，成本可控） |

### 11.2 潜在改进方向

```typescript
// 1. 工具结果自动压缩（已有部分实现）
// 在 tools.ts 中，超过 TURN_END_RESULT_CAP_TOKENS (3000) 的结果被压缩
// 可以进一步改为智能压缩——保留关键信息，丢弃详细输出

// 2. 增量前缀更新
// 当前 addTool 导致完整前缀变更
// 理论上可以通过工具插槽（slot）机制实现部分前缀更新

// 3. 跨会话缓存复用
// 同项目的不同会话可以共享系统提示词的缓存
// 需要确保提示词完全相同（包括工程生命周期配置）

// 4. 更积极的预检
// 当前预检在 95% 触发紧急截断
// 可以在 80% 时预压缩即将超限的工具结果
```

---

## 十二、对我们的项目的启示

### 12.1 可借鉴的设计模式

1. **前缀指纹（Fingerprint）** — 我们已经实现 `build_frozen()`，可以在此基础上添加 SHA256 指纹检测，用于调试和验证缓存是否被意外破坏

2. **Append-Only 日志** — Agent 的消息历史系统应严格遵循只追加原则，任何原位修改（如编辑旧工具结果）都会破坏前缀缓存

3. **双层上下文管理** — 不可变前缀 + 可变历史的分离设计，与我们的 `frozen` + `dynamic` 提示词分区思路一致

4. **工具调用修复流水线** — 尤其是从 reasoning_content 中打捞工具调用的 Scavenge 机制，对 DeepSeek 模型尤为重要

### 12.2 差异化分析

| 维度 | Reasonix | 我们的项目 |
|------|----------|-----------|
| 模型支持 | 仅 DeepSeek | 多模型 |
| 缓存策略 | 前缀缓存（LLM API） | 本地 prompt 缓存 |
| 平台 | CLI/TUI + Desktop | 微信/钉钉/Web |
| 语言 | TypeScript | Rust |
| 并行工具 | Promise.allSettled | tokio::spawn |

### 12.3 可直接采用的代码片段

**前缀指纹检测**（用于验证缓存稳定性）：
```typescript
function computePrefixFingerprint(system: string, tools: ToolSpec[]): string {
  return createHash("sha256")
    .update(JSON.stringify({ system, tools }))
    .digest("hex")
    .slice(0, 16);
}
```

**从思考内容打捞工具调用**（Scavenge 模式）：
```typescript
function scavengeToolCallsFromReasoning(reasoning: string): ToolCall[] {
  // 从 reasoning_content 中正则匹配并提取 DSML 格式的工具调用
  const pattern = /<invoke name="(\w+)">\s*([\s\S]*?)\s*<\/invoke>/g;
  // JSON 解析参数后返回
}
```

---

## 十三、附录

### 13.1 参考文件清单

| 文件 | 作用 |
|------|------|
| `src/loop.ts` | 主循环 CacheFirstLoop 实现 |
| `src/client.ts` | DeepSeek API 客户端 |
| `src/memory/runtime.ts` | 三层上下文分区实现 |
| `src/context-manager.ts` | 上下文折叠策略 |
| `src/tokenizer.ts` | DeepSeek V4 分词器 |
| `src/repair/index.ts` | 工具调用修复编排 |
| `src/telemetry/stats.ts` | 定价、成本计算、会话统计 |
| `src/prompt-fragments.ts` | 共享提示词片段 |
| `src/loop/thinking.ts` | 思考模式适配 |
| `src/loop/messages.ts` | 消息构建 |
| `src/loop/escalation.ts` | 自动升级机制 |
| `src/loop/healing.ts` | 消息修复 |
| `src/types.ts` | 核心类型定义 |
| `docs/ARCHITECTURE.md` | 架构设计文档 |
| `scripts/probe-cache.mjs` | 缓存命中率探测脚本 |
| `benchmarks/real-world-cache/README.md` | 真实用户缓存案例 |

### 13.2 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DEEPSEEK_API_KEY` | — | API 密钥 |
| `DEEPSEEK_BASE_URL` | `https://api.deepseek.com` | API 端点 |
| `REASONIX_FOLD_THRESHOLD` | `0.5` | 上下文折叠触发阈值 |
| `REASONIX_FOLD_TAIL_FRACTION` | `0.2` | 折叠后保留尾部比例 |
| `REASONIX_FORCE_SUMMARY_THRESHOLD` | `0.8` | 强制摘要阈值 |
| `REASONIX_PARALLEL_MAX` | `3` | 并行工具调用最大数量 |
| `REASONIX_TOOL_DISPATCH` | `auto` | 工具调度模式（`auto`/`serial`） |
| `REASONIX_MEMORY` | — | 设为 `off` 禁用记忆系统 |
| `REASONIX_STORM_THRESHOLD` | — | 风暴抑制阈值 |
| `REASONIX_STORM_WINDOW` | — | 风暴检测窗口大小 |