/**
 * ChatPanel.tsx - 聊天面板主组件
 * 
 * 【文件功能概述】
 * 该组件是聊天界面的核心容器，负责管理聊天会话的完整生命周期，包括消息发送、接收、
 * 流式渲染、会话管理、模型选择等功能。它是用户与 AI 助手交互的主要界面入口。
 * - 开发此页面务必适配多语言支持（i18n）
 */

import { useState, useRef, useCallback, useEffect } from 'react'
import {
  ChevronDown,
  Code2,
  Puzzle,
  Brain,
  Cpu,
  Blocks,
  Settings,
  User,
  Paperclip,
  Mic,
  ArrowUp,
  Square,
  ArrowDownToLine,
  Terminal,
  Clock,
  FileText,
  Folder,
  Loader2,
  CheckCircle,
  AlertCircle,
  Check,
  Maximize2,
  Minimize2,
  Sun,
  Moon,
  PanelRightClose,
} from 'lucide-react'
import { ChatMessages, type MessageData } from './ChatMessages'
import { ContextRing } from '@/components/ui/ContextRing'
import { CacheStatsBadge } from '@/components/ui/CacheStatsBadge'
import { ApprovalDialog } from '@/components/ui/ApprovalDialog'
import { TreeBrowser } from './TreeBrowser'
import { compressImage } from '@/lib/imageCompress'
import { startChatStream, cancelChatStream, useApi, queryMentions, expandMentions, getApiBase } from '@/hooks/useApi'
import { useChat } from '@/contexts/ChatContext'
import { useTranslation } from 'react-i18next'
import { useTheme } from '@/contexts/ThemeContext'

import openaiIcon from '@/assets/OpenAI.svg'
import lmStudioIcon from '@/assets/lm-studio.png'
import ollamaIcon from '@/assets/ollama.png'
import deepseekIcon from '@/assets/DeepSeek.png'
import anthropicIcon from '@/assets/Anthropic.png'
import zhipuIcon from '@/assets/zhipu.png'
import xiaomiIcon from '@/assets/Xiaomi.png'
import bailianIcon from '@/assets/bailian.png'

const tools = [
  { id: 'editor', nameKey: 'dashboard.editor', icon: Code2, iconColor: 'text-emerald-400' },
  { id: 'skills', nameKey: 'dashboard.skills', icon: Puzzle, iconColor: 'text-violet-400' },
  { id: 'model', nameKey: 'dashboard.model', icon: Cpu, iconColor: 'text-blue-400' },
  { id: 'agent', nameKey: 'dashboard.agent', icon: Brain, iconColor: 'text-amber-400' },
  { id: 'mcp', nameKey: 'dashboard.mcp', icon: Blocks, iconColor: 'text-cyan-400' },
  { id: 'terminal', nameKey: 'dashboard.terminal', icon: Terminal, iconColor: 'text-green-400' },
  { id: 'schedule', nameKey: 'dashboard.schedule', icon: Clock, iconColor: 'text-orange-400' },
  { id: 'logs', nameKey: 'dashboard.logs', icon: FileText, iconColor: 'text-foreground/50' },
  { id: 'settings', nameKey: 'dashboard.settings', icon: Settings, iconColor: 'text-foreground/50' },
]

interface ModelOption {
  name: string        // 模型名称，如 "zai-org/glm-4.6v-flash"
  providerId: string  // 提供商 ID，用于匹配图标
  contextWindow: number // 模型上下文窗口大小（如 DeepSeek V4 为 1_000_000）
}

// 根据提供商名称匹配图标
function getProviderIcon(providerId: string): string | undefined {
  const id = providerId.toLowerCase().replace(/[\s_-]/g, '')
  if (id.includes('openai') || id === 'openai') return openaiIcon
  if (id.includes('lmstudio') || id.includes('lm_studio') || id.includes('lm-studio')) return lmStudioIcon
  if (id.includes('ollama')) return ollamaIcon
  if (id.includes('deepseek')) return deepseekIcon
  if (id.includes('anthropic') || id === 'anthropic') return anthropicIcon
  if (id.includes('zhipu') || id.includes('zhipuai') || id.includes('glm') || id.includes('智谱')) return zhipuIcon
  if (id.includes('xiaomi') || id.includes('mimo')) return xiaomiIcon
  if (id.includes('aliyun') || id.includes('coding') || id.includes('bailian')) return bailianIcon
  return undefined
}

interface ChatPanelProps {
  onOpenFilePanel?: () => void
  onOpenTool?: (tool: string) => void
  workspacePath?: string
  onWorkspacePathChange?: (path: string) => void
  onToggleConsole?: () => void
  consoleCollapsed?: boolean
  onToggleFilePanel?: () => void
  onToggleTerminal?: () => void
  terminalOpen?: boolean
}

// ---- Helpers ----
function genId() {
  return `msg_${crypto.randomUUID()}`
}

export function ChatPanel({ onOpenFilePanel, onOpenTool, workspacePath, onWorkspacePathChange, onToggleConsole, consoleCollapsed, onToggleFilePanel, onToggleTerminal, terminalOpen }: ChatPanelProps) {
  const { t } = useTranslation()
  const { theme, toggle: toggleTheme } = useTheme()
  const { currentSession, setCurrentSession, messages: contextMessages, refreshSessionList, defaultModelName, setDefaultModelName } = useChat()
  const sessionIdRef = useRef<string | undefined>(undefined)
  const userContentRef = useRef('') // 保存用户消息用于标题生成

  // 同步 currentSession 到 ref，避免 useCallback 依赖变化
  useEffect(() => {
    sessionIdRef.current = currentSession?.id
  }, [currentSession])

  // 切换会话时恢复 selectedModel 和 contextWindow
  useEffect(() => {
    if (currentSession?.model) {
      setSelectedModel(currentSession.model)
    }
  }, [currentSession])

  const [modelOpen, setModelOpen] = useState(false)
  const [toolsOpen, setToolsOpen] = useState(false)
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<MessageData[]>([])
  // 会话累计输入 Token（用于上下文用量环形进度条）
  const [sessionInputTokens, setSessionInputTokens] = useState(0)
  // 当前模型的上下文窗口大小（如 DeepSeek V4 为 1_000_000）
  const [modelContextWindow, setModelContextWindow] = useState(0)
  // 缓存命中率（0~1，仅 DeepSeek 等支持缓存统计的模型）
  const [cacheHitRate, setCacheHitRate] = useState(0)
  // 本次缓存命中 Token 数
  const [lastCacheHitTokens, setLastCacheHitTokens] = useState(0)
  // 待审批的命令执行请求
  const [pendingApproval, setPendingApproval] = useState<{
    id: string; command: string; description: string; toolName: string
  } | null>(null)
  // @-mention 相关状态
  const [mentionOpen, setMentionOpen] = useState(false)
  const [mentionItems, setMentionItems] = useState<{ name: string; path: string; is_dir: boolean }[]>([])
  const [mentionIndex, setMentionIndex] = useState(0)
  const mentionQueryRef = useRef('')
  const currentSessionIdRef = useRef(currentSession?.id)
  const lastSyncMsgCountRef = useRef(0)

  // 当切换会话时：立即清空本地消息和 Token 统计，防止显示旧会话数据
  useEffect(() => {
    const newId = currentSession?.id
    if (currentSessionIdRef.current !== newId) {
      // 只在切换已有会话时清空，首次创建 session（undefined → 有值）时不清
      if (currentSessionIdRef.current !== undefined && newId !== undefined) {
        currentSessionIdRef.current = newId
        lastSyncMsgCountRef.current = 0
        setMessages([])
        setSessionInputTokens(0)
        setCacheHitRate(0)
        setLastCacheHitTokens(0)
      } else {
        currentSessionIdRef.current = newId
      }
    }
  }, [currentSession])

  // 当 contextMessages 实际更新时：同步到本地状态
  useEffect(() => {
    // 流式刚结束时跳过同步，防止 contextMessages 覆盖本地流式消息
    if (streamingJustEndedRef.current) {
      streamingJustEndedRef.current = false
      lastSyncMsgCountRef.current = contextMessages.length
      return
    }
    if (!currentSession) {
      setMessages([])
      return
    }
    if (contextMessages.length > 0 && contextMessages.length !== lastSyncMsgCountRef.current) {
      lastSyncMsgCountRef.current = contextMessages.length

      // 将历史消息转换为 MessageData[]，包括 tool_calls 展开为 agent_step
      const converted: MessageData[] = []
      for (const m of contextMessages) {
        const role = m.role === 'tool' || m.role === 'system' ? 'assistant' : m.role as 'user' | 'assistant'

        // 处理工具调用消息（assistant 消息携带 tool_calls，也可能同时有 first_reasoning 和 again_reasonings）
        if (m.tool_calls && m.tool_calls.length > 0) {
          // 1️⃣ 先处理 first_reasoning（首次思考），必须放在最前面！
          if (m.first_reasoning && m.first_reasoning.trim()) {
            const blocks = m.first_reasoning
              .split(/<｜end▁of▁thinking｜>| response/i)
              .map(s => s.trim())
              .filter(s => s.length > 0)
            if (blocks.length > 0) {
              // 第一个思考块 → first_thought（琥珀色）
              converted.push({
                id: `${m.id}_first_reasoning`,
                role: 'agent_step',
                content: blocks[0],
                agentStep: {
                  stepType: 'first_thought',
                  content: blocks[0],
                  toolName: undefined,
                  toolResult: undefined,
                  turn: 0,
                  maxTurns: 20,
                }
              })
              // 后续思考块 → 放在后面（如果有的话）
              for (let fi = 1; fi < blocks.length; fi++) {
                if (blocks[fi] && blocks[fi].trim()) {
                  converted.push({
                    id: `${m.id}_first_reasoning_${fi}`,
                    role: 'agent_step',
                    content: blocks[fi],
                    agentStep: {
                      stepType: 'thought',
                      content: blocks[fi],
                      toolName: undefined,
                      toolResult: undefined,
                      turn: fi,
                      maxTurns: 20,
                    }
                  })
                }
              }
            }
          }
          // 2️⃣ 然后展开 tool_calls 为 agent_step 消息（思考之后！）
          for (const tc of m.tool_calls) {
            converted.push({
              id: `tool_${tc.id}`,
              role: 'agent_step',
              content: `调用工具: ${tc.name}`,
              agentStep: {
                stepType: 'tool_call',
                content: tc.arguments || '{}',
                toolName: tc.name,
                toolResult: undefined,
                turn: 0,
                maxTurns: 20,
              }
            })
          }
          // 3️⃣ 处理 again_reasonings（工具调用完成后的二次思考）
          if (m.again_reasonings && m.again_reasonings.length > 0) {
            for (let ri = 0; ri < m.again_reasonings.length; ri++) {
              const r = m.again_reasonings[ri]
              if (r && r.trim()) {
                converted.push({
                  id: `${m.id}_again_reasoning_${ri}`,
                  role: 'agent_step',
                  content: r,
                  agentStep: {
                    stepType: 'thought',
                    content: r,
                    toolName: undefined,
                    toolResult: undefined,
                    turn: ri + 1,
                    maxTurns: 20,
                  }
                })
              }
            }
          }
          // 4️⃣ 最后添加 assistant 消息本身（最终回复）
          // ⚠️ 如果在此处新增字段，请同步更新下方分支 2、分支 4 以及流式 done 处的对应字段
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
        }
        // 处理推理/思考内容（first_reasoning 和 again_reasonings 字段，无 tool_calls）
        else if (role === 'assistant' && (m.first_reasoning || m.again_reasonings || m.reasoning)) {
          // 处理首次思考（first_reasoning → first_thought）
          if (m.first_reasoning && m.first_reasoning.trim()) {
            // 解析多个思考块（兼容合并的思考内容）
            const firstReasoningBlocks = m.first_reasoning
              .split(/<｜end▁of▁thinking｜>/i)
              .map(s => s.trim())
              .filter(s => s.length > 0)
            
            if (firstReasoningBlocks.length > 0) {
              // 第一个思考块 → first_thought（主要样式）
              converted.push({
                id: `${m.id}_first_reasoning`,
                role: 'agent_step',
                content: firstReasoningBlocks[0],
                agentStep: {
                  stepType: 'first_thought',
                  content: firstReasoningBlocks[0],
                  toolName: undefined,
                  toolResult: undefined,
                  turn: 0,
                  maxTurns: 20,
                }
              })
              
              // first_reasoning 中的后续思考块 → thought（次要样式）
              for (let fi = 1; fi < firstReasoningBlocks.length; fi++) {
                const fb = firstReasoningBlocks[fi]
                if (fb && fb.trim()) {
                  converted.push({
                    id: `${m.id}_first_reasoning_${fi}`,
                    role: 'agent_step',
                    content: fb,
                    agentStep: {
                      stepType: 'thought',
                      content: fb,
                      toolName: undefined,
                      toolResult: undefined,
                      turn: fi,
                      maxTurns: 20,
                    }
                  })
                }
              }
            }
          }
          // 处理后续思考（again_reasonings → thought）
          if (m.again_reasonings && m.again_reasonings.length > 0) {
            for (let ri = 0; ri < m.again_reasonings.length; ri++) {
              const r = m.again_reasonings[ri]
              if (r && r.trim()) {
                converted.push({
                  id: `${m.id}_again_reasoning_${ri}`,
                  role: 'agent_step',
                  content: r,
                  agentStep: {
                    stepType: 'thought',
                    content: r,
                    toolName: undefined,
                    toolResult: undefined,
                    turn: ri + 1,
                    maxTurns: 20,
                  }
                })
              }
            }
          }
          // 兼容旧字段 reasoning（如果 first_reasoning 和 again_reasonings 都为空）
          if (!m.first_reasoning && !m.again_reasonings && m.reasoning && m.reasoning.trim()) {
            converted.push({
              id: `${m.id}_reasoning`,
              role: 'agent_step',
              content: m.reasoning,
              agentStep: {
                stepType: 'first_thought',
                content: m.reasoning,
                toolName: undefined,
                toolResult: undefined,
                turn: 0,
                maxTurns: 20,
              }
            })
          }
          // 添加 assistant 消息本身（剥离 <think> 标签，避免在 ChatMessages 中重复渲染）
          if (m.content && m.content.trim()) {
            const strippedContent = m.content
              // 移除完整的 <think>...</think> 块
              .replace(/<think\s*>[\s\S]*?<\/think\s*>/gi, '')
              // 移除不完整的 <think> 开头（流式垃圾数据）
              .replace(/<think\s*>[\s\S]*$/i, '')
              // 移除 Google Gemma 风格的 <|channel|>thought...<channel|>
              .replace(/<\|channel\|?>thought[\s\S]*?<channel\|>/gi, '')
              .trim()
            if (strippedContent) {
              // ⚠️ 如果在此处新增字段，请同步更新分支 1、分支 4 以及流式 done 处的对应字段
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
          }
        }
        // 处理工具结果消息（role=tool）
        else if (m.role === 'tool' && m.tool_call_id) {
          converted.push({
            id: `result_${m.tool_call_id}`,
            role: 'agent_step',
            content: m.content,
            agentStep: {
              stepType: 'tool_result',
              content: '',
              toolName: m.tool_name,
              toolResult: m.content,
              turn: 0,
              maxTurns: 20,
            }
          })
        }
        // 普通消息
        else {
          // ⚠️ 如果在此处新增字段，请同步更新分支 1、分支 2 以及流式 done 处的对应字段
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
            imagePaths: (m as any).image_paths?.length > 0 ? (m as any).image_paths : undefined,
            sessionId: (m as any).image_paths?.length > 0 ? m.session_id : undefined,
          } as any)
        }
      }
      // 合并：保留本地已有的 tool_call 状态（toolResult、stepType），
      // 避免 contextMessages 同步时覆盖流式累积的执行输出
      setMessages(prev => {
        if (prev.length === 0) return converted
        // 对每条转换后的消息，检查本地是否有同名 ID 且带有 toolResult 的记录
        const merged = converted.map(msg => {
          if (msg.role !== 'agent_step' || !msg.agentStep) return msg
          if (msg.agentStep.stepType !== 'tool_call') return msg
          const local = prev.find(p => p.id === msg.id)
          if (local?.agentStep?.toolResult || local?.agentStep?.stepType === 'tool_call_done' || local?.agentStep?.stepType === 'tool_error') {
            return local // 保留本地的累积状态
          }
          return msg
        })
        return merged
      })

      // 从历史消息中获取累计 inputTokens（后端已存储为累计值，取最后一条 assistant 消息即可）
      const lastAssistantMsg = [...converted].reverse().find(
        m => m.role === 'assistant' && m.inputTokens && m.inputTokens > 0
      )
      if (lastAssistantMsg) {
        setSessionInputTokens(lastAssistantMsg.inputTokens ?? 0)
      }

      // 从最后一条 assistant 消息中恢复缓存命中率统计
      const lastAssistant = [...converted].reverse().find(
        m => m.role === 'assistant' && (m as any).cacheHitRate !== undefined && (m as any).cacheHitRate !== null && (m as any).cacheHitRate >= 0
      )
      if (lastAssistant) {
        const rate = Number((lastAssistant as any).cacheHitRate)
        if (rate >= 0) setCacheHitRate(rate)
        const tokens = (lastAssistant as any).cachedTokens
        if (typeof tokens === 'number' && tokens > 0) setLastCacheHitTokens(tokens)
      }
    }
  }, [contextMessages, currentSession])

  // 同步 workspaceName：当 workspacePath 变化时更新显示名
  useEffect(() => {
    if (workspacePath) {
      setWorkspaceName(workspacePath.split(/[/\\]/).pop() || 'workspace')
    } else {
      // localStorage 为空时从后端获取默认工作目录
      fetch(`${getApiBase()}/paths`)
        .then(r => r.json())
        .then(body => {
          if (body.success && body.data?.workspace_dir) {
            const ws = body.data.workspace_dir
            onWorkspacePathChange?.(ws)
            setWorkspaceName(ws.split(/[/\\]/).pop() || 'workspace')
          } else {
            setWorkspaceName('未设置工作目录')
          }
        })
        .catch(() => setWorkspaceName('未设置工作目录'))
    }
  }, [workspacePath])

  const [isStreaming, setIsStreaming] = useState(false)
  const [streamingContent, setStreamingContent] = useState('')
  const [streamingReasoning, setStreamingReasoning] = useState('')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [showScrollBtn, setShowScrollBtn] = useState(false)
  // 从 localStorage 同步初始化，避免异步加载默认模型前的空窗期
  const [selectedModel, setSelectedModel] = useState(() => localStorage.getItem('jeeves-default-model') || '')
  const [modelOptions, setModelOptions] = useState<ModelOption[]>([])
  const [selectedAgent, setSelectedAgent] = useState(() => localStorage.getItem('jeeves-selected-agent') || '')
  const [agentProfiles, setAgentProfiles] = useState<{ id: string; name: string }[]>([])
  const [agentOpen, setAgentOpen] = useState(false)
  const [workspaceName, setWorkspaceName] = useState('')
  const [workspaceOpen, setWorkspaceOpen] = useState(false)
  const [showTreeBrowser, setShowTreeBrowser] = useState(false)
  const [pendingImages, setPendingImages] = useState<string[]>([])
  const imageInputRef = useRef<HTMLInputElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const messagesEndRef = useRef<HTMLDivElement | null>(null)
  const scrollContainerRef = useRef<HTMLDivElement | null>(null)
  const streamingContentRef = useRef('')
  const streamingReasoningRef = useRef('')
  const hasFlushedFirstReasoningRef = useRef(false) // 标记是否已刷新第一次思考内容
  const streamingJustEndedRef = useRef(false) // 标记流式刚结束，阻止 contextMessages 覆盖
  const [isRethinking, setIsRethinking] = useState(false) // 标记是否处于二次思考阶段
  const [subagentActivity, setSubagentActivity] = useState<{
    id: string
    name: string
    task: string
    status: 'running' | 'done' | 'error'
  } | null>(null)

  const handleInput = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value
    setInput(value)
    const el = e.target
    el.style.height = 'auto'
    el.style.height = el.scrollHeight + 'px'

    // 检测 @-mention
    const match = value.match(/(?:^|\s)@([^\s]*)$/)
    if (match) {
      const query = match[1]
      mentionQueryRef.current = query
      setMentionOpen(true)
      setMentionIndex(0)
      void queryMentions(workspacePath, query).then(items => {
        setMentionItems(items)
      })
    } else {
      setMentionOpen(false)
      setMentionItems([])
    }
  }, [workspacePath])

  const scrollToBottom = useCallback((smooth = true) => {
    if (messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto' })
    }
  }, [])

  // Auto scroll when new messages arrive or streaming
  useEffect(() => {
    if (!showScrollBtn) {
      scrollToBottom(false)
    }
  }, [messages, streamingContent, showScrollBtn, scrollToBottom])

  // Scroll handler – show button when scrolled up
  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
    setShowScrollBtn(distFromBottom > 100)
  }, [])

  const { abortRef, listProviders, getDefaultModel, setDefaultModel, updateSessionModel } = useApi()

  // Load model list from backend
  const loadModels = useCallback(() => {
    listProviders().then(providers => {
      if (providers && providers.length > 0) {
        const options: ModelOption[] = []
        for (const p of providers) {
          for (const m of p.models) {
            const modelName = typeof m === 'string' ? m : m.name
            const modelCw = typeof m === 'object' ? (m.context_window ?? 0) : 0
            options.push({ name: modelName, providerId: p.name, contextWindow: modelCw })
          }
        }
        setModelOptions(options)

        // 异步加载模型上下文窗口大小
        fetch(`${getApiBase()}/models`).then(r => r.json()).then(body => {
          if (body.success && Array.isArray(body.data)) {
            const windowMap: Record<string, number> = {}
            for (const mdl of body.data) {
              if (mdl.context_window) {
                windowMap[mdl.name] = mdl.context_window
              }
            }
            setModelOptions(prev => prev.map(opt => ({
              ...opt,
              contextWindow: windowMap[opt.name] || 0
            })))
          }
        }).catch(() => {})
      }
    }).catch(() => {})
    // 页面加载时同时获取后端保存的默认模型
    getDefaultModel().then(defaultModelName => {
      if (defaultModelName) {
        setDefaultModelName(defaultModelName)
      }
    }).catch(() => {})

    // 加载智能体列表
    fetch(`${getApiBase()}/agents`).then(r => r.json()).then(body => {
      if (body.success && Array.isArray(body.data)) {
        const profiles: { id: string; name: string }[] = body.data.map((a: any) => ({ id: a.id, name: a.name }))
        setAgentProfiles(profiles)
      }
    }).catch(() => {})
  }, [listProviders, getDefaultModel, setDefaultModelName])

  // Load workspace info（后端无 workspace 端点，默认即可）

  // Load model list on mount
  useEffect(() => {
    loadModels()
  }, [loadModels])

  // Sync selectedModel with ChatContext default
  useEffect(() => {
    if (defaultModelName && !selectedModel && !currentSession?.model) {
      setSelectedModel(defaultModelName)
    }
  }, [defaultModelName, selectedModel, currentSession?.model])

  // 当 selectedModel 或 modelOptions 变化时更新上下文窗口大小
  useEffect(() => {
    if (selectedModel && modelOptions.length > 0) {
      const opt = modelOptions.find(o => o.name === selectedModel)
      setModelContextWindow(opt?.contextWindow || 0)
    }
  }, [selectedModel, modelOptions])

  // Save default model when user changes selection
  const handleModelChange = useCallback((modelName: string) => {
    setSelectedModel(modelName)
    // 更新全局默认模型（IM 等后台场景也使用默认模型）
    setDefaultModel(modelName).catch(() => {})
    // 更新当前会话的模型（避免前端跨会话泄漏）
    if (currentSession?.id) {
      setCurrentSession({ ...currentSession, model: modelName })
      updateSessionModel(currentSession.id, modelName).catch(() => {})
    }
    // 更新上下文窗口
    const opt = modelOptions.find(o => o.name === modelName)
    setModelContextWindow(opt?.contextWindow || 0)
    // 切换模型时重置累计 Token 和缓存统计
    setSessionInputTokens(0)
    setCacheHitRate(0)
    setLastCacheHitTokens(0)
  }, [modelOptions, currentSession, setCurrentSession, updateSessionModel, setDefaultModel])

  // 获取当前选中模型对应的图标
  const selectedModelIcon = useCallback(() => {
    if (!selectedModel) return undefined
    const opt = modelOptions.find(o => o.name === selectedModel)
    return opt ? getProviderIcon(opt.providerId) : undefined
  }, [selectedModel, modelOptions])

  // 从用户消息中提取简洁标题
  function makeTitle(text: string): string {
    let title = text.replace(/[\r\n]+/g, ' ').trim()
    // 移除常见问候前缀
    title = title.replace(/^(你好|hello|hi|hey|您好)[\s,，!！\.]*/i, '')
    // 截断到合理长度
    if (title.length > 50) {
      title = title.slice(0, 47) + '...'
    }
    return title || t('chat.newConversation')
  }

  // SSE streaming via HTTP POST + SSE
  const startStreaming = useCallback((userContent: string, images: string[] = []) => {
    setIsStreaming(true)
    setStreamingContent('')
    setStreamingReasoning('')
    setStreamError(null)
    streamingContentRef.current = ''
    streamingReasoningRef.current = ''
    hasFlushedFirstReasoningRef.current = false
    setIsRethinking(false)
    userContentRef.current = userContent

    const sessionId = sessionIdRef.current

    const ac = startChatStream(
      {
        message: userContent,
        model: selectedModel || undefined,
        session_id: sessionId,
        workspace: workspacePath || undefined,
        images: images.length > 0 ? images : undefined,
        agent_id: selectedAgent || undefined, // 空 = 默认智能体（不用 profile 覆盖）
      },
      {
        onChunk: (chunk) => {
          streamingContentRef.current += chunk
          // 流式内容不管切不切会话都要更新（会话恢复时需要）
          setStreamingContent(streamingContentRef.current)

          // 从流式内容中提取 <think> 标签内容，实时显示思考过程
          const thinkMatch = streamingContentRef.current.match(/<think\s*>([\s\S]*?)(?:<\/think\s*>|$)/)
          if (thinkMatch) {
            const extractedReasoning = thinkMatch[1].trim()
            if (extractedReasoning && extractedReasoning !== streamingReasoningRef.current) {
              streamingReasoningRef.current = extractedReasoning
              setStreamingReasoning(extractedReasoning)
            }
          }
        },
        onDone: (result: { content?: string; sessionId?: string }) => {
          setIsStreaming(false)

          // 流式结束：固化剩余的推理内容（安全兜底）
          if (streamingReasoningRef.current.trim()) {
            const stepType = hasFlushedFirstReasoningRef.current ? 'thought' : 'first_thought'
            setMessages(prev => [...prev, {
              id: genId(),
              role: 'agent_step',
              content: streamingReasoningRef.current,
              agentStep: {
                stepType,
                content: streamingReasoningRef.current,
                toolName: undefined,
                toolResult: undefined,
                turn: 0,
                maxTurns: 20,
              }
            }])
            if (stepType === 'first_thought') {
              hasFlushedFirstReasoningRef.current = true
            }
            streamingReasoningRef.current = ''
            setStreamingReasoning('')
          }

          // 固化最终文本为 assistant 消息（携带 Token 用量）
          // ⚠️ 如果在此处新增字段，请同步更新上方 useEffect 中分支 1、2、4 的对应字段
          const content = streamingContentRef.current || result.content || ''
          if (content) {
            setMessages(prev => [...prev, { id: genId(), role: 'assistant', content, inputTokens: (result as any).inputTokens, outputTokens: (result as any).outputTokens, cachedTokens: (result as any).cachedTokens, lastInputTokens: (result as any).lastInputTokens, lastOutputTokens: (result as any).lastOutputTokens, cacheHitRate: (result as any).cache_hit_rate ?? (result as any).cacheHitRate }])
          }
          setStreamingContent('')
          streamingContentRef.current = ''

          // 安全兜底：如果有仍处于 tool_call 状态的步骤，标记为完成（防止 tool_result 事件丢失）
          setMessages(prev => prev.map(m => {
            if (m.role === 'agent_step' && m.agentStep?.stepType === 'tool_call') {
              return {
                ...m,
                agentStep: {
                  ...m.agentStep!,
                  stepType: 'tool_call_done',
                }
              }
            }
            return m
          }))

          // 更新会话累计 Token（用于上下文用量环形进度条）
          // 使用后端计算的累计值 cumulative_input_tokens（后端已跨轮次累加好），
          // 前端直接使用，无需自行累加
          const cumulativeInput = (result as any).cumulativeInputTokens
          if (typeof cumulativeInput === 'number' && cumulativeInput > 0) {
            setSessionInputTokens(cumulativeInput)
          }

          // 更新缓存命中率统计
          const hitRate = (result as any).cache_hit_rate ?? (result as any).cacheHitRate
          if (typeof hitRate === 'number' && hitRate >= 0) {
            setCacheHitRate(hitRate)
          }
          const hitTokens = (result as any).cache_hit_tokens ?? (result as any).cacheHitTokens
          if (typeof hitTokens === 'number' && hitTokens > 0) {
            setLastCacheHitTokens(hitTokens)
          }

          // 首次对话：更新 session_id
          if (result.sessionId && result.sessionId !== sessionIdRef.current) {
            const title = makeTitle(userContentRef.current)
            setCurrentSession({
              id: result.sessionId,
              name: title,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
              model: selectedModel === 'Auto' ? '' : selectedModel,
            })
            sessionIdRef.current = result.sessionId
            // 刷新侧边栏会话列表
            refreshSessionList()
          }
          streamingJustEndedRef.current = true
        },
        onError: (err) => {
          setIsStreaming(false)
          setStreamingContent('')
          streamingContentRef.current = ''
          setStreamError(err || t('chat.connectionFailed'))
          streamingJustEndedRef.current = true
        },
        onAgentStep: (step) => {
          if (step.stepType === 'reasoning') {
            streamingReasoningRef.current += step.content
            setStreamingReasoning(streamingReasoningRef.current)

          } else if (step.stepType === 'first_thought' || step.stepType === 'thought') {
            const reasoningContent = streamingReasoningRef.current.trim() || step.content
            if (reasoningContent) {
              setMessages(prev => [...prev, {
                id: genId(),
                role: 'agent_step',
                content: reasoningContent,
                agentStep: {
                  stepType: step.stepType,
                  content: reasoningContent,
                  toolName: undefined,
                  toolResult: undefined,
                  turn: step.turn,
                  maxTurns: step.maxTurns,
                }
              }])
              if (step.stepType === 'first_thought') {
                hasFlushedFirstReasoningRef.current = true
              }
            }
            streamingReasoningRef.current = ''
            setStreamingReasoning('')

          } else if (step.stepType === 'tool_call') {
            setMessages(prev => {
              const newMessages = [...prev]
              
              // 检查是否已有相同 toolName 的步骤（首次 delta 可能已发送过的）
              const existingIdx = newMessages.findIndex(
                m => m.role === 'agent_step' && m.agentStep?.toolName === step.toolName && m.agentStep?.stepType === 'tool_call'
              )
              if (existingIdx >= 0) {
                // 更新已有步骤的参数内容
                newMessages[existingIdx] = {
                  ...newMessages[existingIdx],
                  content: step.content,
                  agentStep: {
                    ...newMessages[existingIdx].agentStep!,
                    content: step.content,
                  }
                }
                return newMessages
              }
              
              const contentToFlush = streamingContentRef.current.trim()
              if (contentToFlush) {
                newMessages.push({ id: genId(), role: 'assistant', content: contentToFlush })
              }
              
              newMessages.push({
                id: genId(),
                role: 'agent_step',
                content: step.content,
                agentStep: {
                  stepType: 'tool_call',
                  content: step.content,
                  toolName: step.toolName,
                  toolResult: undefined,
                  turn: step.turn,
                  maxTurns: step.maxTurns,
                }
              })
              
              return newMessages
            })
            
            streamingContentRef.current = ''
            setStreamingContent('')
            setIsRethinking(true)

          } else if (step.stepType === 'tool_chunk') {
            // 检测子 Agent 进度事件
            if (step.content.startsWith('{"type":"subagent"')) {
              try {
                const evt = JSON.parse(step.content)
                if (evt.type === 'subagent') {
                  if (evt.action === 'start') {
                    const name = evt.agent_id || ''
                    setSubagentActivity({ id: name, name, task: evt.task || '', status: 'running' })
                  } else if (evt.action === 'done') {
                    setSubagentActivity(prev => prev ? { ...prev, status: evt.error ? 'error' : 'done' } : null)
                    // 3 秒后自动移除已完成的状态
                    setTimeout(() => setSubagentActivity(null), 3000)
                  }
                }
              } catch {}
              return // 不追加到 tool_result
            }
            // 实时追加终端输出块到对应的工具消息中
            setMessages(prev => {
              const updated = [...prev]
              for (let i = updated.length - 1; i >= 0; i--) {
                const m = updated[i]
                if (
                  m.role === 'agent_step' &&
                  (m.agentStep?.stepType === 'tool_call' || m.agentStep?.stepType === 'tool_call_done') &&
                  m.agentStep?.toolName === step.toolName
                ) {
                  updated[i] = {
                    ...m,
                    agentStep: {
                      ...m.agentStep!,
                      toolResult: (m.agentStep!.toolResult || '') + step.content,
                    }
                  }
                  break
                }
              }
              return updated
            })

          } else if (step.stepType === 'tool_result') {
            setMessages(prev => {
              const updated = [...prev]
              for (let i = updated.length - 1; i >= 0; i--) {
                const m = updated[i]
                if (
                  m.role === 'agent_step' &&
                  m.agentStep?.stepType === 'tool_call' &&
                  m.agentStep?.toolName === step.toolName
                ) {
                  // 优先使用 toolResult 字段（实际输出），
                  // 如果已经通过 tool_chunk 累积了内容则保留
                  const rawResult = (step as any).toolResult ?? (step as any).tool_result
                  const existingResult = m.agentStep!.toolResult || rawResult || ''
                  updated[i] = {
                    ...m,
                    agentStep: {
                      ...m.agentStep!,
                      stepType: 'tool_call_done',
                      toolResult: existingResult,
                    },
                  }
                  break
                }
              }
              return updated
            })

          } else if (step.stepType === 'tool_error') {
            setMessages(prev => {
              const updated = [...prev]
              for (let i = updated.length - 1; i >= 0; i--) {
                const m = updated[i]
                if (
                  m.role === 'agent_step' &&
                  m.agentStep?.stepType === 'tool_call' &&
                  m.agentStep?.toolName === step.toolName
                ) {
                  const rawResult = (step as any).tool_result
                  const existingResult = m.agentStep!.toolResult || rawResult || ''
                  updated[i] = {
                    ...m,
                    agentStep: {
                      ...m.agentStep!,
                      stepType: 'tool_error',
                      toolResult: existingResult,
                    },
                  }
                  break
                }
              }
              return updated
            })

          } else if (step.stepType === 'retry') {
            setMessages(prev => [...prev, {
              id: genId(),
              role: 'agent_step',
              content: step.content,
              agentStep: {
                stepType: 'retry',
                content: step.content,
                toolName: undefined,
                toolResult: undefined,
                turn: step.turn,
                maxTurns: step.maxTurns,
              }
            }])
          } else if (step.stepType === 'approval_required') {
            const approvalId = step.approval_id
            const command = step.approval?.arguments || step.content
            if (approvalId && command) {
              setPendingApproval({
                id: approvalId,
                command: command,
                description: step.approval?.message || '',
                toolName: step.toolName || 'execute_command',
              })
            }
            setIsStreaming(false)
          }
        },
      },
    )

    abortRef.current = ac
  }, [selectedModel, abortRef, workspacePath, selectedAgent])

  const handleSend = useCallback(() => {
    if ((!input.trim() && pendingImages.length === 0) || isStreaming) return

    let msg = input.trim() || '请描述这张图片'
    // 展开 @-mention 引用为文件内容
    if (msg.includes('@')) {
      expandMentions(msg, workspacePath).then(expanded => {
        sendMessage(expanded)
      }).catch(() => {
        sendMessage(msg)
      })
    } else {
      sendMessage(msg)
    }
  }, [input, isStreaming, pendingImages, workspacePath])

  // sendMessage 引用（通过 useEffect 同步到 ref，供审批对话框直接调用）
  const sendMessage = useCallback((msg: string) => {
    const userMsg: MessageData = {
      id: genId(),
      role: 'user',
      content: msg,
      images: pendingImages.length > 0 ? pendingImages : undefined,
    }
    setMessages(prev => [...prev, userMsg])
    const imgs = [...pendingImages]
    setInput('')
    setMentionOpen(false)
    setPendingImages([])
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
    startStreaming(msg, imgs)
  }, [startStreaming, pendingImages])

  // 打断停止：AbortController 取消 SSE 请求 + 通知后端
  const handleStop = useCallback(() => {
    // 先拿到当前 sessionId（可能有值）
    const sid = sessionIdRef.current
    // 通知后端取消 Agent 执行（后端会通过 SSE 返回 stopped 事件）
    if (sid) {
      void cancelChatStream(sid)
    }
    // 不要在收到后端 stopped 响应前关闭 SSE 连接！
    // abortRef 由 startChatStream 内部管理，SSE 流结束后自动清理
  }, [])


  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // @-mention 键盘导航
      if (mentionOpen && mentionItems.length > 0) {
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setMentionIndex(i => Math.min(i + 1, mentionItems.length - 1))
          return
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setMentionIndex(i => Math.max(i - 1, 0))
          return
        }
        if (e.key === 'Enter' && mentionItems[mentionIndex]) {
          e.preventDefault()
          insertMention(mentionItems[mentionIndex])
          return
        }
        if (e.key === 'Escape') {
          setMentionOpen(false)
          return
        }
      }
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault()
        handleSend()
      }
    },
    [handleSend, mentionOpen, mentionItems, mentionIndex]
  )

  /** 插入 @-mention 选择到输入框 */
  const insertMention = useCallback((item: { name: string; path: string; is_dir: boolean }) => {
    const el = textareaRef.current
    if (!el) return
    const cursorPos = el.selectionStart ?? el.value.length
    const before = el.value.slice(0, cursorPos)
    const after = el.value.slice(cursorPos)
    const atPos = before.lastIndexOf('@')
    if (atPos === -1) return
    const mentionText = item.is_dir ? `@${item.path}/ ` : `@${item.path} `
    const newValue = before.slice(0, atPos) + mentionText + after
    setInput(newValue)
    setMentionOpen(false)
    setMentionItems([])
    requestAnimationFrame(() => {
      const pos = atPos + mentionText.length
      el.setSelectionRange(pos, pos)
      el.focus()
    })
  }, [])

  // Close workspace popup on outside click
  useEffect(() => {
    if (!workspaceOpen) return
    const handler = () => setWorkspaceOpen(false)
    document.addEventListener('click', handler)
    return () => document.removeEventListener('click', handler)
  }, [workspaceOpen])

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      abortRef.current?.abort()
      abortRef.current = null
    }
  }, [abortRef])

  const hasMessages = messages.length > 0 || isStreaming

  return (
    <div className="h-full flex flex-col bg-mainbg min-w-0">
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="relative">
          <div
            className="flex items-center gap-1.5 cursor-pointer hover:opacity-80 transition-opacity"
            onClick={(e) => { e.stopPropagation(); setWorkspaceOpen(!workspaceOpen) }}
          >
            <span className="text-sm font-medium text-foreground/90">{workspaceName}</span>
            <ChevronDown className="w-3.5 h-3.5 text-foreground/50" />
          </div>
          {workspaceOpen && (
            <div className="absolute left-0 top-full mt-1 w-56 py-1 rounded-md bg-card border border-border shadow-lg z-20">
              <button
                className="w-full flex items-center gap-2 px-3 py-2 text-xs text-foreground/70 hover:bg-foreground/10 transition-colors"
                onClick={(e) => {
                  e.stopPropagation()
                  setWorkspaceOpen(false)
                  setShowTreeBrowser(true)
                }}
              >
                <Folder className="w-3.5 h-3.5" />
                浏览目录
              </button>
              {workspacePath && (
                <div className="px-3 py-1.5 border-t border-border mt-1">
                  <div className="text-[10px] text-foreground/40 font-mono truncate" title={workspacePath}>
                    {workspacePath}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
        <div className="flex items-center gap-1">
          {/* 主题切换 */}
          <button onClick={toggleTheme} className="p-1.5 rounded hover:bg-foreground/10 transition-colors" title={theme === 'dark' ? '切换亮色模式' : '切换暗色模式'}>
            {theme === 'dark' ? <Sun className="w-4 h-4 text-foreground/60" /> : <Moon className="w-4 h-4 text-foreground/60" />}
          </button>
          {/* 终端切换 */}
          <button
            onClick={onToggleTerminal}
            className={`p-1.5 rounded hover:bg-foreground/10 transition-colors ${terminalOpen ? 'bg-foreground/10' : ''}`}
            title="终端"
          >
            <Terminal className="w-4 h-4 text-foreground/60" />
          </button>
          {/* 文件面板切换 */}
          <button onClick={onToggleFilePanel} className="p-1.5 rounded hover:bg-foreground/10 transition-colors" title="文件预览">
            <PanelRightClose className="w-4 h-4 text-foreground/60" />
          </button>
          <div className="relative">
            <button
              onClick={() => setToolsOpen(!toolsOpen)}
              className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
            >
              <Settings className="w-4 h-4 text-foreground/60" />
            </button>
            {toolsOpen && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setToolsOpen(false)} />
                <div className="absolute right-0 top-full mt-1 w-40 py-1 rounded-md bg-card border border-border shadow-lg z-20">
                  {tools.map((tool) => {
                    const Icon = tool.icon
                    return (
                      <button
                        key={tool.id}
                        className="w-full flex items-center gap-2 px-3 py-2 text-xs text-foreground/70 hover:bg-foreground/10 transition-colors"
                        onClick={() => {
                          setToolsOpen(false)
                          if (tool.id === 'editor') {
                            onOpenFilePanel?.()
                          } else {
                            onOpenTool?.(tool.id)
                            // 主控台折叠时点击工具自动展开
                            if (consoleCollapsed) {
                              onToggleConsole?.()
                            }
                          }
                        }}
                      >
                        <Icon className={`w-3.5 h-3.5 ${tool.iconColor || ''}`} />
                        {t(tool.nameKey)}
                      </button>
                    )
                  })}
                </div>
              </>
            )}
          </div>
          <button
            onClick={onToggleConsole}
            className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
            title={consoleCollapsed ? '展开主控台' : '折叠主控台'}
          >
            {consoleCollapsed ? (
              <Maximize2 className="w-4 h-4 text-foreground/60" />
            ) : (
              <Minimize2 className="w-4 h-4 text-foreground/60" />
            )}
          </button>
          <button className="p-1.5 rounded hover:bg-foreground/10 transition-colors">
            <User className="w-4 h-4 text-foreground/60" />
          </button>
        </div>
      </div>

      {/* Messages area or brand placeholder */}
      {hasMessages ? (
        <div className="relative flex-1 flex flex-col min-h-0">
          <div
            ref={scrollContainerRef}
            onScroll={handleScroll}
            className="flex-1 overflow-y-auto"
          >
            {/* 任务进度面板（保留扩展点） */}
            <ChatMessages
              messages={messages}
              isStreaming={isStreaming}
              streamingContent={streamingContent}
              streamingReasoning={streamingReasoning}
              isRethinking={isRethinking}
              messagesEndRef={messagesEndRef}
            />

            {/* Error message */}
            {streamError && (
              <div className="px-3 py-2 mx-3 mb-2 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-400">
                {streamError}
              </div>
            )}
          </div>

          {/* Scroll to bottom button */}
          {showScrollBtn && (
            <button
              onClick={() => scrollToBottom(true)}
              className="absolute bottom-2 left-1/2 -translate-x-1/2 p-1.5 rounded-full bg-foreground/10 hover:bg-foreground/20 border border-border transition-colors z-10"
            >
              <ArrowDownToLine className="w-4 h-4 text-foreground/60" />
            </button>
          )}
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-base font-medium text-foreground/40 select-none">
            Jeeves
          </p>
        </div>
      )}

      {/* Bottom input area */}
      <div className="px-3 pb-3 shrink-0">
        <p className="text-[11px] text-foreground/30 mb-2 leading-relaxed text-center">
          {t('chat.chatWith')}
          <br />
          <span className="text-[10px]">@ 可选择文件或文件夹作为上下文</span>
        </p>

        {/* @-mention 下拉菜单 */}
        {mentionOpen && mentionItems.length > 0 && (
          <div className="relative">
            <div className="absolute bottom-full left-0 right-0 mb-1 z-50 max-h-52 overflow-y-auto rounded-lg border border-border bg-card shadow-xl">
              <div className="px-3 py-1.5 text-[11px] text-foreground/50 border-b border-border">
                @ 选择文件作为上下文
              </div>
              {mentionItems.map((item, i) => (
                <button
                  key={item.path}
                  className={`w-full flex items-center gap-2 px-3 py-2 text-xs text-left transition-colors ${
                    i === mentionIndex
                      ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
                      : 'hover:bg-foreground/5 text-foreground/70'
                  }`}
                  onMouseEnter={() => setMentionIndex(i)}
                  onClick={() => {
                    const el = textareaRef.current
                    if (!el) return
                    const cursorPos = el.selectionStart ?? el.value.length
                    const before = el.value.slice(0, cursorPos)
                    const after = el.value.slice(cursorPos)
                    const atPos = before.lastIndexOf('@')
                    if (atPos === -1) return
                    const mentionText = item.is_dir ? `@${item.path}/ ` : `@${item.path} `
                    const newValue = before.slice(0, atPos) + mentionText + after
                    setInput(newValue)
                    setMentionOpen(false)
                    setMentionItems([])
                    requestAnimationFrame(() => {
                      const pos = atPos + mentionText.length
                      el.setSelectionRange(pos, pos)
                      el.focus()
                    })
                  }}
                >
                  {item.is_dir ? (
                    <Folder className="w-4 h-4 text-amber-400 shrink-0" />
                  ) : (
                    <FileText className="w-4 h-4 text-blue-400 shrink-0" />
                  )}
                  <span className="font-mono truncate flex-1">{item.path}</span>
                  {item.is_dir && <span className="text-[10px] text-foreground/40 shrink-0">目录</span>}
                </button>
              ))}
              <div className="px-3 py-1.5 text-[10px] text-foreground/30 border-t border-border">
                ↑↓ 选择 · Enter 确认 · Esc 关闭
              </div>
            </div>
          </div>
        )}

        <div className="rounded-lg bg-foreground/5 border border-border">
          {/* 待发送图片缩略图 */}
          {pendingImages.length > 0 && (
            <div className="px-3 pt-2 flex gap-2 flex-wrap">
              {pendingImages.map((url, i) => (
                <div key={i} className="relative group">
                  <img src={url} className="w-14 h-14 rounded-lg object-cover border border-border" alt="" />
                  <button
                    className="absolute -top-1 -right-1 w-4 h-4 rounded-full bg-red-500 text-white text-[10px] flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                    onClick={() => setPendingImages(prev => prev.filter((_, j) => j !== i))}
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
          )}
          <div className="px-3 pt-2">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={handleInput}
              onKeyDown={handleKeyDown}
              rows={1}
              spellCheck={false}
              placeholder={t('chat.inputPlaceholder')}
              className="w-full bg-transparent text-sm text-foreground/80 placeholder-foreground/30 outline-none resize-none leading-5 py-0.5 max-h-[160px]"
              style={{ height: 'auto' }}
              onPaste={async (e) => {
                const items = e.clipboardData.items
                let hasImage = false
                for (const item of items) {
                  if (item.type.startsWith('image/')) {
                    hasImage = true
                    e.preventDefault()
                    const file = item.getAsFile()
                    if (!file) continue
                    try {
                      const dataUrl = await compressImage(file)
                      setPendingImages(prev => [...prev, dataUrl])
                    } catch { /* skip */ }
                  }
                }
                // If only text was pasted (no image), let default paste happen
                if (hasImage) e.preventDefault()
              }}
              onDrop={async (e) => {
                const files = e.dataTransfer.files
                if (files.length === 0) return
                let hasImage = false
                for (const file of files) {
                  if (file.type.startsWith('image/')) {
                    hasImage = true
                    e.preventDefault()
                    try {
                      const dataUrl = await compressImage(file)
                      setPendingImages(prev => [...prev, dataUrl])
                    } catch { /* skip */ }
                  }
                }
                if (hasImage) e.preventDefault()
              }}
            />
          </div>

          <div className="flex items-center justify-between px-2 pb-2">
            <input
              ref={imageInputRef}
              type="file"
              accept="image/*"
              multiple
              className="hidden"
              onChange={async (e) => {
                const files = e.target.files
                if (!files) return
                for (const file of files) {
                  try {
                    const dataUrl = await compressImage(file)
                    setPendingImages(prev => [...prev, dataUrl])
                  } catch { /* skip */ }
                }
                e.target.value = ''
              }}
            />
            <button
              className="p-1 rounded hover:bg-foreground/10 transition-colors"
              onClick={() => imageInputRef.current?.click()}
              title="添加图片"
            >
              <Paperclip className="w-4 h-4 text-foreground/50" />
            </button>

            <div className="flex items-center gap-1">
              {/* 上下文用量环形进度条（仅 DeepSeek 等支持缓存统计的模型） */}
              {modelContextWindow > 0 && (
                <ContextRing used={sessionInputTokens} total={modelContextWindow} />
              )}
              {/* 缓存命中率统计徽章 */}
              <CacheStatsBadge
                hitRate={cacheHitRate}
                hitTokens={lastCacheHitTokens}
                inputTokens={sessionInputTokens}
              />
              <div className="relative">
                <button
                  onClick={() => { setModelOpen(!modelOpen); loadModels() }}
                  className="flex items-center gap-1 px-3 py-1 rounded text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
                >
                  {selectedModel !== 'Auto' && selectedModelIcon() && (
                    <img src={selectedModelIcon()} className="w-3.5 h-3.5 rounded" alt="" />
                  )}
                  <span>{selectedModel}</span>
                  <ChevronDown className="w-3 h-3 shrink-0" />
                </button>
                {modelOpen && (
                  <>
                    <div className="fixed inset-0 z-10" onClick={() => setModelOpen(false)} />
                    <div className="absolute bottom-full right-0 mb-1 w-60 py-1 rounded-md bg-card border border-border shadow-lg z-20 max-h-64 overflow-y-auto">
                      {modelOptions.map((opt) => (
                        <button
                          key={opt.name}
                          className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left text-foreground/70 hover:bg-foreground/10 transition-colors"
                          onClick={() => {
                            handleModelChange(opt.name)
                            setModelOpen(false)
                          }}
                        >
                          {getProviderIcon(opt.providerId) && (
                            <img src={getProviderIcon(opt.providerId)} className="w-4 h-4 rounded shrink-0" alt="" />
                          )}
                          <span className="flex-1">{opt.name}</span>
                          {opt.name === defaultModelName && (
                            <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                          )}
                        </button>
                      ))}
                    </div>
                  </>
                )}
              </div>

              {/* 智能体选择 */}
              <div className="relative">
                <button
                  onClick={() => setAgentOpen(!agentOpen)}
                  className="flex items-center gap-1 px-2 py-1 rounded text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
                  title={t('chat.selectAgent')}
                >
                  <Brain className="w-3.5 h-3.5" />
                  <span className="max-w-[60px] truncate">
                    {selectedAgent
                      ? (agentProfiles.find(a => a.id === selectedAgent)?.name
                        || (selectedAgent === 'default' ? '默认智能体' : selectedAgent))
                      : t('chat.defaultAgent')}
                  </span>
                  <ChevronDown className="w-3 h-3 shrink-0" />
                </button>
                {agentOpen && (
                  <>
                    <div className="fixed inset-0 z-10" onClick={() => setAgentOpen(false)} />
                    <div className="absolute bottom-full right-0 mb-1 w-44 py-1 rounded-md bg-card border border-border shadow-lg z-20">
                      {agentProfiles.length === 0 ? (
                        <div className="px-3 py-2 text-xs text-foreground/40">{t('chat.noAgents')}</div>
                      ) : (
                        [...agentProfiles].sort((a, b) => a.id === 'default' ? -1 : b.id === 'default' ? 1 : 0).map((agent, _index, sorted) => {
                          const isDefault = agent.id === 'default'
                          const showDivider = isDefault && sorted.length > 1
                          return (
                            <div key={agent.id}>
                              <button
                                className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left text-foreground/70 hover:bg-foreground/10 transition-colors"
                                onClick={() => {
                                  setSelectedAgent(agent.id)
                                  localStorage.setItem('jeeves-selected-agent', agent.id)
                                  fetch(`${getApiBase()}/set-agent/${encodeURIComponent(agent.id)}`)
                                  setAgentOpen(false)
                                }}
                              >
                                <Brain className={`w-3.5 h-3.5 shrink-0 ${isDefault ? 'text-orange-400' : 'text-cyan-400'}`} />
                                <span className="flex-1">{isDefault ? '默认智能体' : agent.name}</span>
                                {agent.id === selectedAgent && <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />}
                              </button>
                              {showDivider && <div className="mx-2 my-1 border-t border-border" />}
                            </div>
                          )
                        })
                      )}
                    </div>
                  </>
                )}
              </div>

              <button className="p-1 rounded hover:bg-foreground/10 transition-colors">
                <Mic className="w-4 h-4 text-foreground/50" />
              </button>
              <button
                onClick={isStreaming ? handleStop : handleSend}
                disabled={!isStreaming && !input.trim() && pendingImages.length === 0}
                className={`p-1.5 rounded-lg transition-colors ${
                  isStreaming
                    ? 'bg-red-500 hover:bg-red-400 animate-breathing'
                    : 'bg-green-500 hover:bg-green-400 disabled:opacity-40 disabled:cursor-not-allowed'
                }`}
              >
                {isStreaming ? (
                  <Square className="w-4 h-4 text-white" />
                ) : (
                  <ArrowUp className="w-4 h-4 text-black" />
                )}
              </button>
            </div>
          </div>
        </div>
        {/* 子 Agent 工作状态卡片 */}
        {subagentActivity && (
          <div className="px-1 pt-2">
            <div className={`flex items-center gap-2 px-3 py-2 rounded-lg border text-xs ${
              subagentActivity.status === 'running'
                ? 'bg-cyan-500/5 border-cyan-500/20'
                : subagentActivity.status === 'error'
                  ? 'bg-red-500/5 border-red-500/20'
                  : 'bg-green-500/5 border-green-500/20'
            }`}>
              {subagentActivity.status === 'running' ? (
                <Loader2 className="w-3.5 h-3.5 text-cyan-400 animate-spin shrink-0" />
              ) : subagentActivity.status === 'error' ? (
                <AlertCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />
              ) : (
                <CheckCircle className="w-3.5 h-3.5 text-green-400 shrink-0" />
              )}
              <span className={subagentActivity.status === 'running' ? 'text-cyan-300/80' : subagentActivity.status === 'error' ? 'text-red-300/80' : 'text-green-300/80'}>
                {subagentActivity.status === 'running'
                  ? `🔄 ${subagentActivity.name}: ${subagentActivity.task.slice(0, 60)}${subagentActivity.task.length > 60 ? '...' : ''}`
                  : subagentActivity.status === 'error'
                    ? `✗ ${subagentActivity.name}: 执行失败`
                    : `✓ ${subagentActivity.name}: 任务完成`
                }
              </span>
            </div>
          </div>
        )}
      </div>
      <style>{`
        @keyframes breathing {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.6; transform: scale(0.92); }
        }
        .animate-breathing {
          animation: breathing 2.5s ease-in-out infinite;
        }
      `}</style>
      {showTreeBrowser && (
        <TreeBrowser
          initialPath={workspacePath || '/'}
          onSelect={(path) => {
            onWorkspacePathChange?.(path)
            setWorkspaceName(path.split(/[/\\]/).pop() || 'workspace')
            setShowTreeBrowser(false)
          }}
          onCancel={() => setShowTreeBrowser(false)}
        />
      )}
      {/* 命令执行审批对话框 — 直接调用 /chat/approve API 释放阻塞的 runtime */}
      <ApprovalDialog
        pending={pendingApproval}
        onClose={() => setPendingApproval(null)}
      />
    </div>
  )
}
