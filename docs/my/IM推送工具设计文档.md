# IM 推送工具设计文档

## 概述

新增 `im_push` 工具，让 LLM 可以主动向指定 IM 机器人推送消息，同时配合定时任务实现定时推送功能。

---

## 工具设计

### `im_push` 工具

```json
{
  "name": "im_push",
  "description": "Send a message to an IM platform via a specific bot. Use to proactively push notifications, alerts, or scheduled messages to users or groups. If 'robot' is omitted, uses the current session's bot or the default one.",
  "parameters": {
    "type": "object",
    "properties": {
      "platform": {
        "type": "string",
        "enum": ["dingtalk"],
        "description": "IM platform to send to"
      },
      "robot": {
        "type": "string",
        "description": "Bot instance name (e.g. 'my-bot-1', 'default'). In session context, leave empty to use the current bot. In cron tasks, specify which bot to use."
      },
      "target_type": {
        "type": "string",
        "enum": ["private", "group"],
        "description": "Send to a private user or a group chat"
      },
      "target_id": {
        "type": "string",
        "description": "Recipient ID. For private: user's userId. For group: openConversationId."
      },
      "content": {
        "type": "string",
        "description": "Message content (plain text or markdown)"
      },
      "content_type": {
        "type": "string",
        "enum": ["text", "markdown"],
        "description": "Message format type (default: text)"
      },
      "title": {
        "type": "string",
        "description": "Message title (required when content_type is 'markdown')"
      }
    },
    "required": ["platform", "target_type", "target_id", "content"]
  }
}
```

### 机器人注册架构

多机器人场景下，每个 IM 平台可注册多个机器人实例，通过名称区分：

```
IM_GATEWAY
  └─ Registry (平台 → 机器人列表)
       ├─ dingtalk →
       │    ├─ "my-bot-1"   → DingTalkAdapter(机器人A)
       │    ├─ "my-bot-2"   → DingTalkAdapter(机器人B)
       │    └─ "default"    → DingTalkAdapter(默认)
       ├─ feishu → ...
       └─ ...
```

```rust
// 注册时指定机器人名称
registry.register_bot(name, adapter);

// 获取时指定机器人名称，不指定则返回第一个可用
gateway.get_adapter(platform, Some("my-bot-1")).await
```

### `robot` 字段行为

| 场景 | robot 值 | 行为 |
|------|----------|------|
| 对话中推送 | 不填 | 使用当前会话绑定的机器人 |
| 对话中推送 | "my-bot-2" | 使用指定机器人 |
| 定时任务 | "my-bot-1" | cron 到点后 Agent 用指定机器人发 |
| 定时任务 | 不填 | 使用该平台的默认机器人 |

---

## 配合定时任务使用

```cron
action: create
name: "每日天气推送"
schedule: "0 8 * * *"
payload: "查询北京天气，然后调用 im_push 发送到我的钉钉私聊"

# cron 到点 → Agent 执行 payload → 自动调 im_push
```

---

## 文件修改清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `backend/src/tools/builtin/im_push.rs` | 新建 | `im_push` 工具实现 |
| `backend/src/tools/builtin/mod.rs` | 修改 | 注册 `im_push` 工具 |

## 补充说明

1. **机器人注册**：`IMGateway` 需要支持按名称注册/查找适配器
2. **target_id 获取**：用户 ID 从对话消息中提取，或在配置中指定
3. **群聊权限**：机器人需配置群聊消息发送权限
4. **频率限制**：钉钉 API 有调用频率限制
5. **content_type=markdown**：需同时提供 title 字段
