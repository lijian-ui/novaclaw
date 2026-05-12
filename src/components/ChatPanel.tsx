import { useState, useRef, useCallback, useEffect } from 'react'
import {
  ChevronDown,
  Plus,
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
  ListTodo,
} from 'lucide-react'
import { ChatMessages, type MessageData } from './ChatMessages'
import { TaskList, type TaskProgress, type TaskProgressItem } from './TaskList'
import { useApi } from '@/hooks/useApi'
import { useChat } from '@/contexts/ChatContext'
import { useTranslation } from 'react-i18next'

import openaiIcon from '@/assets/OpenAI.svg'
import lmStudioIcon from '@/assets/lm-studio.png'
import ollamaIcon from '@/assets/ollama.png'
import deepseekIcon from '@/assets/DeepSeek.png'

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
}

// 根据提供商名称匹配图标
function getProviderIcon(providerId: string): string | undefined {
  const id = providerId.toLowerCase().replace(/[\s_-]/g, '')
  if (id.includes('openai') || id === 'openai') return openaiIcon
  if (id.includes('lmstudio') || id.includes('lm_studio') || id.includes('lm-studio')) return lmStudioIcon
  if (id.includes('ollama')) return ollamaIcon
  if (id.includes('deepseek')) return deepseekIcon
  return undefined
}

interface ChatPanelProps {
  onOpenFilePanel?: () => void
  onOpenTool?: (tool: string) => void
}

// ---- Helpers ----
let mockIdCounter = 0
function genId() {
  return `msg_${++mockIdCounter}_${Date.now()}`
}

export function ChatPanel({ onOpenFilePanel, onOpenTool }: ChatPanelProps) {
  const { t } = useTranslation()
  const { currentSession, setCurrentSession, messages: contextMessages } = useChat()
  const sessionIdRef = useRef<string | undefined>(undefined)
  const userContentRef = useRef('') // 保存用户消息用于标题生成

  // 同步 currentSession 到 ref，避免 useCallback 依赖变化
  useEffect(() => {
    sessionIdRef.current = currentSession?.id
  }, [currentSession])

  const [modelOpen, setModelOpen] = useState(false)
  const [toolsOpen, setToolsOpen] = useState(false)
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<MessageData[]>([])
  const currentSessionIdRef = useRef(currentSession?.id)
  const lastSyncMsgCountRef = useRef(0)

  // 当切换会话时：立即清空本地消息，防止显示旧会话数据
  useEffect(() => {
    const newId = currentSession?.id
    if (currentSessionIdRef.current !== newId) {
      // 只在切换已有会话时清空，首次创建 session（undefined → 有值）时不清
      if (currentSessionIdRef.current !== undefined && newId !== undefined) {
        currentSessionIdRef.current = newId
        lastSyncMsgCountRef.current = 0
        setMessages([])
      } else {
        currentSessionIdRef.current = newId
      }
    }
  }, [currentSession])

  // 当 contextMessages 实际更新时：同步到本地状态
  useEffect(() => {
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

        // 处理工具调用消息（assistant 消息携带 tool_calls，也可能同时有 first_reasoning）
        if (m.tool_calls && m.tool_calls.length > 0) {
          // 先处理 first_reasoning（思考内容），放在 tool_call 之前显示
          if (m.first_reasoning && m.first_reasoning.trim()) {
            const blocks = m.first_reasoning
              .split(/ response/i)
              .map(s => s.trim())
              .filter(s => s.length > 0)
            if (blocks.length > 0) {
              // 第一个思考块 → first_thought
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
              // 后续思考块 → thought
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
          // 再添加 assistant 消息本身（如有内容）
          if (m.content && m.content.trim()) {
            converted.push({
              id: m.id,
              role,
              content: m.content,
            })
          }
          // 展开 tool_calls 为 agent_step 消息
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
        }
        // 处理推理/思考内容（first_reasoning 和 reasonings 字段，无 tool_calls）
        else if (role === 'assistant' && (m.first_reasoning || m.reasonings || m.reasoning)) {
          // 使用 parseAllThinkBlocks 解析 first_reasoning，分离多个思考块
          // 第一个思考块 → first_thought（主要样式），后续思考块 → thought（次要样式）
          const rawFirstReasoning = m.first_reasoning || m.reasoning || ''
          // 解析多个思考块（兼容合并的思考内容）
          const firstReasoningBlocks = rawFirstReasoning
            .split(/<｜end▁of▁thinking｜>/i)
            .map(s => s.trim())
            .filter(s => s.length > 0)
          
          // 处理第一次思考块（first_reasoning 中的第一块 → first_thought）
          if (firstReasoningBlocks.length > 0) {
            // 第一个思考块 → first_thought
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
            
            // 后续思考块 → thought（次要样式）
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
          // 处理后续思考（reasonings）
          if (m.reasonings && m.reasonings.length > 0) {
            for (let ri = 0; ri < m.reasonings.length; ri++) {
              const r = m.reasonings[ri]
              if (r && r.trim()) {
                converted.push({
                  id: `${m.id}_reasoning_${ri}`,
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
          // 兼容旧字段 reasoning（如果 first_reasoning 和 reasonings 都为空）
          if (!m.first_reasoning && !m.reasonings && m.reasoning && m.reasoning.trim()) {
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
              converted.push({
                id: m.id,
                role: 'assistant',
                content: strippedContent,
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
          converted.push({
            id: m.id,
            role,
            content: m.content,
          })
        }
      }
      setMessages(converted)
    }
  }, [contextMessages, currentSession])
  const [isStreaming, setIsStreaming] = useState(false)
  const [streamingContent, setStreamingContent] = useState('')
  const [streamingReasoning, setStreamingReasoning] = useState('')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [showScrollBtn, setShowScrollBtn] = useState(false)
  const [selectedModel, setSelectedModel] = useState('Auto')
  const [modelOptions, setModelOptions] = useState<ModelOption[]>([{ name: 'Auto', providerId: '' }])
  const [workspaceName, setWorkspaceName] = useState('workspace')
  const [workspaceOpen, setWorkspaceOpen] = useState(false)
  const [taskProgress, setTaskProgress] = useState<TaskProgress | null>(null)
  const [taskDetected, setTaskDetected] = useState<boolean | null>(null)
  const folderInputRef = useRef<HTMLInputElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const messagesEndRef = useRef<HTMLDivElement | null>(null)
  const scrollContainerRef = useRef<HTMLDivElement | null>(null)
  const streamingContentRef = useRef('')
  const streamingReasoningRef = useRef('')
  const hasFlushedFirstReasoningRef = useRef(false) // 标记是否已刷新第一次思考

  const handleInput = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value)
    const el = e.target
    el.style.height = 'auto'
    el.style.height = el.scrollHeight + 'px'
  }, [])

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

  const { connectChatStream, sendChatMessage, stopChatStream, disconnectChat, listProviders, getDefaultModel, setDefaultModel } = useApi()

  // Load model list from backend
  const loadModels = useCallback(() => {
    listProviders().then(providers => {
      if (providers && providers.length > 0) {
        const options: ModelOption[] = [{ name: 'Auto', providerId: '' }]
        for (const p of providers) {
          for (const m of p.models) {
            options.push({ name: m, providerId: p.name })
          }
        }
        setModelOptions(options)
      }
    }).catch(() => {
      // Backend offline, keep ['Auto']
    })
  }, [listProviders])

  // Load workspace info（后端无 workspace 端点，默认即可）

  // Initial load - load both model list and default model
  useEffect(() => {
    loadModels()
    getDefaultModel().then(modelName => {
      if (modelName) {
        setSelectedModel(modelName)
      }
    }).catch(() => {})
  }, [loadModels, getDefaultModel])

  // Save default model when user changes selection
  const handleModelChange = useCallback((modelName: string) => {
    setSelectedModel(modelName)
    setDefaultModel(modelName === 'Auto' ? '' : modelName).catch(() => {})
  }, [setDefaultModel])

  // 获取当前选中模型对应的图标
  const selectedModelIcon = useCallback(() => {
    if (selectedModel === 'Auto') return undefined
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

  // Pure WebSocket streaming – no mock fallback
  const startStreaming = useCallback((userContent: string) => {
    setTaskDetected(null)
    setTaskProgress(null)
    setIsStreaming(true)
    setStreamingContent('')
    setStreamingReasoning('')
    setStreamError(null)
    streamingContentRef.current = ''
    streamingReasoningRef.current = ''
    hasFlushedFirstReasoningRef.current = false
    userContentRef.current = userContent

    const ws = connectChatStream(
      null,
      (chunk) => {
        streamingContentRef.current += chunk
        setStreamingContent(streamingContentRef.current)

        // 从流式内容中提取 <think> 标签内容，实时显示思考过程
        // 某些模型（如 DeepSeek）将思考内容放在 <think> 标签中，没有独立的 reasoning_content 字段
        const thinkMatch = streamingContentRef.current.match(/<think\s*>([\s\S]*?)(?:<\/think\s*>|$)/)
        if (thinkMatch) {
          const extractedReasoning = thinkMatch[1].trim()
          if (extractedReasoning && extractedReasoning !== streamingReasoningRef.current) {
            streamingReasoningRef.current = extractedReasoning
            setStreamingReasoning(extractedReasoning)
          }
        }
      },
      (result: { content?: string; sessionId?: string }) => {
        setIsStreaming(false)

        // 流式结束：把剩余的推理内容固化为 agent_step 消息
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
          streamingReasoningRef.current = ''
          setStreamingReasoning('')
        }

        // 固化最终文本输出为 assistant 消息
        const content = streamingContentRef.current || result.content || ''
        if (content) {
          setMessages(prev => [...prev, { id: genId(), role: 'assistant', content }])
        }
        setStreamingContent('')
        streamingContentRef.current = ''

        // 首次对话：后端返回新 session_id，更新 ChatContext
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
        }
      },
      (err) => {
        setIsStreaming(false)
        setStreamingContent('')
        streamingContentRef.current = ''
        setStreamError(err || t('chat.connectionFailed'))
      },
      (step) => {
        if (step.stepType === 'reasoning') {
          // reasoning 类型：流式累积，实时显示在 ThinkingBlock 里（打字机效果）
          streamingReasoningRef.current += step.content
          setStreamingReasoning(streamingReasoningRef.current)

        } else if (step.stepType === 'first_thought' || step.stepType === 'thought') {
          // first_thought/thought 类型：直接作为 agent_step 消息固化显示
          // 固化之前的流式推理内容
          if (streamingReasoningRef.current.trim()) {
            setMessages(prev => [...prev, {
              id: genId(),
              role: 'agent_step',
              content: streamingReasoningRef.current,
              agentStep: {
                stepType: hasFlushedFirstReasoningRef.current ? 'thought' : 'first_thought',
                content: streamingReasoningRef.current,
                toolName: undefined,
                toolResult: undefined,
                turn: step.turn,
                maxTurns: step.maxTurns,
              }
            }])
            streamingReasoningRef.current = ''
            setStreamingReasoning('')
          }
          // 固化流式文本
          if (streamingContentRef.current.trim()) {
            setMessages(prev => [...prev, { id: genId(), role: 'assistant', content: streamingContentRef.current }])
            streamingContentRef.current = ''
            setStreamingContent('')
          }
          // 固化当前思考消息
          setMessages(prev => [...prev, {
            id: genId(),
            role: 'agent_step',
            content: step.content,
            agentStep: {
              stepType: step.stepType,
              content: step.content,
              toolName: step.toolName,
              toolResult: undefined,
              turn: step.turn,
              maxTurns: step.maxTurns,
            }
          }])
          // 标记首次思考已刷新
          if (step.stepType === 'first_thought') {
            hasFlushedFirstReasoningRef.current = true
          }

        } else if (step.stepType === 'tool_call') {
          // 工具调用开始：先把当前推理内容固化为 agent_step 消息
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
            streamingReasoningRef.current = ''
            setStreamingReasoning('')
            hasFlushedFirstReasoningRef.current = true
          }
          // 同时清空流式文本（工具调用前的文本已固化）
          if (streamingContentRef.current.trim()) {
            setMessages(prev => [...prev, { id: genId(), role: 'assistant', content: streamingContentRef.current }])
            streamingContentRef.current = ''
            setStreamingContent('')
          }
          // 追加 tool_call 消息（显示为工具调用卡片，done=false 表示执行中）
          setMessages(prev => [...prev, {
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
          }])

        } else if (step.stepType === 'tool_result') {
          // 工具执行完成：把最后一个同名 tool_call 标记为 done
          setMessages(prev => {
            const updated = [...prev]
            // 从后往前找最近的同名 tool_call，更新为 done 状态
            for (let i = updated.length - 1; i >= 0; i--) {
              const m = updated[i]
              if (
                m.role === 'agent_step' &&
                m.agentStep?.stepType === 'tool_call' &&
                m.agentStep?.toolName === step.toolName
              ) {
                updated[i] = {
                  ...m,
                  agentStep: { ...m.agentStep!, stepType: 'tool_call_done' }
                }
                break
              }
            }
            return updated
          })

        } else if (step.stepType === 'retry') {
          // 重试提示
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
        } else if (step.stepType === 'task_detection') {
          // 复杂任务检测结果
          try {
            const detection = JSON.parse(step.content)
            setTaskDetected(detection.is_complex)
          } catch {
            console.error('Failed to parse task detection:', step.content)
          }
        } else if (step.stepType === 'task_plan') {
          // 任务计划解析
          try {
            const plan = JSON.parse(step.content) as TaskProgress
            setTaskProgress(plan)
          } catch {
            console.error('Failed to parse task plan:', step.content)
          }
        } else if (step.stepType === 'task_progress') {
          // 任务进度更新
          try {
            const progress = JSON.parse(step.content) as TaskProgress
            setTaskProgress(progress)
          } catch {
            console.error('Failed to parse task progress:', step.content)
          }
        }
        // tool_error 等其他类型忽略（不影响主流程）
      },
    )

    // 获取当前 session_id（若无则用 undefined 让后端自动创建）
    const sessionId = sessionIdRef.current

    // 等待 WebSocket 连接建立后再发送消息
    if (ws.readyState === WebSocket.OPEN) {
      sendChatMessage(userContent, selectedModel === 'Auto' ? undefined : selectedModel, sessionId)
    } else if (ws.readyState === WebSocket.CONNECTING) {
      ws.addEventListener('open', () => {
        sendChatMessage(userContent, selectedModel === 'Auto' ? undefined : selectedModel, sessionId)
      }, { once: true })
    } else {
      setIsStreaming(false)
      setStreamError(t('chat.webSocketFailed'))
    }

    return () => {
      disconnectChat()
    }
  }, [connectChatStream, sendChatMessage, disconnectChat, selectedModel])

  const handleSend = useCallback(() => {
    if (!input.trim() || isStreaming) return

    const userMsg: MessageData = {
      id: genId(),
      role: 'user',
      content: input.trim(),
    }
    setMessages(prev => [...prev, userMsg])
    const msg = input.trim()
    setInput('')
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }

    startStreaming(msg)
  }, [input, isStreaming, startStreaming])

  // 打断停止：发送 stop 指令给后端 → 后端取消 LLM 请求 → 后端返回 "stopped" → onDone 处理
  const handleStop = useCallback(() => {
    stopChatStream()
  }, [stopChatStream])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault()
        handleSend()
      }
    },
    [handleSend]
  )

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
      disconnectChat()
    }
  }, [disconnectChat])

  const hasMessages = messages.length > 0 || isStreaming

  return (
    <div className="h-full flex flex-col bg-mainbg min-w-0">
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="relative">
          <input
            ref={folderInputRef}
            type="file"
            className="hidden"
            /* @ts-ignore */
            webkitdirectory=""
            directory=""
            onChange={async (e) => {
              const files = e.target.files
              if (files && files.length > 0) {
                const relPath = files[0].webkitRelativePath
                const dirName = relPath.split('/')[0]
                setWorkspaceName(dirName)
                setWorkspaceOpen(false)
                // 打开文件预览面板
                onOpenFilePanel?.()
              }
            }}
          />
          <div
            className="flex items-center gap-1.5 cursor-pointer hover:opacity-80 transition-opacity"
            onClick={(e) => { e.stopPropagation(); setWorkspaceOpen(!workspaceOpen) }}
          >
            <span className="text-sm font-medium text-foreground/90">{workspaceName}</span>
            <ChevronDown className="w-3.5 h-3.5 text-foreground/50" />
          </div>
          {workspaceOpen && (
            <div className="absolute left-0 top-full mt-1 w-44 py-1 rounded-md bg-card border border-border shadow-lg z-20">
              <button
                className="w-full flex items-center gap-2 px-3 py-2 text-xs text-foreground/70 hover:bg-foreground/10 transition-colors"
                onClick={(e) => {
                  e.stopPropagation()
                  const input = folderInputRef.current
                  if (input) {
                    input.setAttribute('webkitdirectory', '')
                    input.setAttribute('directory', '')
                    input.click()
                  }
                }}
              >
                <Folder className="w-3.5 h-3.5" />
                {t('chat.openFolder')}
              </button>
            </div>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button className="p-1.5 rounded hover:bg-foreground/10 transition-colors">
            <Plus className="w-4 h-4 text-foreground/60" />
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
            <TaskList 
              taskProgress={taskProgress} 
              onClose={() => setTaskProgress(null)}
              onTaskClick={(task: TaskProgressItem) => {
                console.log('Task clicked:', task)
              }}
            />
            {/* 复杂任务检测指示器 */}
            {taskDetected === true && !taskProgress && (
              <div className="mx-3 mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-gradient-to-r from-green-500/10 to-emerald-500/10 border border-green-500/20">
                <ListTodo className="w-4 h-4 text-green-400" />
                <span className="text-sm text-green-400/90">检测到复杂任务，正在生成任务清单...</span>
              </div>
            )}
            <ChatMessages
              messages={messages}
              isStreaming={isStreaming}
              streamingContent={streamingContent}
              streamingReasoning={streamingReasoning}
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
            NovaClaw AI Agent
          </p>
        </div>
      )}

      {/* Bottom input area */}
      <div className="px-3 pb-3 shrink-0">
        <p className="text-[11px] text-foreground/30 mb-2 leading-relaxed text-center">
          {t('chat.chatWith')}
        </p>
        <div className="rounded-lg bg-foreground/5 border border-border">
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
            />
          </div>

          <div className="flex items-center justify-between px-2 pb-2">
            <button className="p-1 rounded hover:bg-foreground/10 transition-colors">
              <Paperclip className="w-4 h-4 text-foreground/50" />
            </button>

            <div className="flex items-center gap-1">
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
                    <div className="absolute bottom-full right-0 mb-1 w-60 max-h-40 overflow-y-auto py-1 rounded-md bg-card border border-border shadow-lg z-20">
                      {modelOptions.map((opt) => (
                        <button
                          key={opt.name}
                          className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left text-foreground/70 hover:bg-foreground/10 transition-colors"
                          onClick={() => {
                            handleModelChange(opt.name)
                            setModelOpen(false)
                          }}
                        >
                          {opt.name === 'Auto' ? (
                            <span className="text-foreground/50">Auto</span>
                          ) : (
                            <>
                              {getProviderIcon(opt.providerId) && (
                                <img src={getProviderIcon(opt.providerId)} className="w-4 h-4 rounded shrink-0" alt="" />
                              )}
                              <span>{opt.name}</span>
                            </>
                          )}
                        </button>
                      ))}
                    </div>
                  </>
                )}
              </div>

              <button className="p-1 rounded hover:bg-foreground/10 transition-colors">
                <Mic className="w-4 h-4 text-foreground/50" />
              </button>
              <button
                onClick={isStreaming ? handleStop : handleSend}
                disabled={!isStreaming && !input.trim()}
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
    </div>
  )
}
