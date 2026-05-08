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
  ArrowDownToLine,
  Terminal,
  Clock,
  FileText,
  Folder,
} from 'lucide-react'
import { ChatMessages, type MessageData } from './ChatMessages'
import { useApi } from '@/hooks/useApi'
import { useChat } from '@/contexts/ChatContext'
import type { Session } from '@/types'
import openaiIcon from '@/assets/OpenAI.svg'
import lmStudioIcon from '@/assets/lm-studio.png'
import ollamaIcon from '@/assets/ollama.png'
import deepseekIcon from '@/assets/DeepSeek.png'

const tools = [
  { id: 'editor', name: '编辑器', icon: Code2, iconColor: 'text-emerald-400' },
  { id: 'skills', name: '技能', icon: Puzzle, iconColor: 'text-violet-400' },
  { id: 'model', name: '模型', icon: Cpu, iconColor: 'text-blue-400' },
  { id: 'agent', name: '智能体', icon: Brain, iconColor: 'text-amber-400' },
  { id: 'mcp', name: 'MCP', icon: Blocks, iconColor: 'text-cyan-400' },
  { id: 'terminal', name: '终端', icon: Terminal, iconColor: 'text-green-400' },
  { id: 'schedule', name: '定时任务', icon: Clock, iconColor: 'text-orange-400' },
  { id: 'logs', name: '日志', icon: FileText, iconColor: 'text-foreground/50' },
  { id: 'settings', name: '设置', icon: Settings, iconColor: 'text-foreground/50' },
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
  const { currentSession, setCurrentSession } = useChat()
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
  const [isStreaming, setIsStreaming] = useState(false)
  const [streamingContent, setStreamingContent] = useState('')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [showScrollBtn, setShowScrollBtn] = useState(false)
  const [selectedModel, setSelectedModel] = useState('Auto')
  const [modelOptions, setModelOptions] = useState<ModelOption[]>([{ name: 'Auto', providerId: '' }])
  const [workspaceName, setWorkspaceName] = useState('workspace')
  const [workspaceOpen, setWorkspaceOpen] = useState(false)
  const folderInputRef = useRef<HTMLInputElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const messagesEndRef = useRef<HTMLDivElement | null>(null)
  const scrollContainerRef = useRef<HTMLDivElement | null>(null)
  const streamingContentRef = useRef('')

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

  const { connectChatStream, sendChatMessage, disconnectChat, listProviders, getDefaultModel, setDefaultModel } = useApi()

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
    return title || '新对话'
  }

  // Pure WebSocket streaming – no mock fallback
  const startStreaming = useCallback((userContent: string) => {
    setIsStreaming(true)
    setStreamingContent('')
    setStreamError(null)
    streamingContentRef.current = ''
    userContentRef.current = userContent // 保存用户消息用于标题

    const ws = connectChatStream(
      null,
      (chunk) => {
        streamingContentRef.current += chunk
        setStreamingContent(streamingContentRef.current)
      },
      (result: { content?: string; sessionId?: string }) => {
        setIsStreaming(false)
        const content = streamingContentRef.current || result.content || ''
        if (content) {
          setMessages(prev => [...prev, { id: genId(), role: 'assistant', content }])
        }
        // 如果后端返回了新的 session_id（首次对话自动创建），更新 ChatContext
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
        setStreamingContent('')
        streamingContentRef.current = ''
      },
      (err) => {
        setIsStreaming(false)
        setStreamingContent('')
        streamingContentRef.current = ''
        setStreamError(err || '对话连接失败，请检查后端服务是否运行')
      },
      (step) => {
        // Add agent step as a special message
        setMessages(prev => [...prev, {
          id: genId(),
          role: 'agent_step',
          content: step.content,
          agentStep: {
            stepType: step.stepType,
            content: step.content,
            toolName: step.toolName,
            toolResult: step.toolResult,
            turn: step.turn,
            maxTurns: step.maxTurns,
          }
        }])
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
      setStreamError('WebSocket 连接失败')
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
                  // 通过 DOM 设置 directory 属性，避免 React 忽略非标准属性
                  const input = folderInputRef.current
                  if (input) {
                    input.setAttribute('webkitdirectory', '')
                    input.setAttribute('directory', '')
                    input.click()
                  }
                }}
              >
                <Folder className="w-3.5 h-3.5" />
                打开文件夹
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
                        {tool.name}
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
            <ChatMessages
              messages={messages}
              isStreaming={isStreaming}
              streamingContent={streamingContent}
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
          您正在与 NovaClaw 聊天
        </p>
        <div className="rounded-lg bg-foreground/5 border border-border">
          <div className="px-3 pt-2">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={handleInput}
              onKeyDown={handleKeyDown}
              rows={1}
              placeholder="输入消息..."
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
                onClick={handleSend}
                disabled={isStreaming || !input.trim()}
                className="p-1.5 rounded-lg bg-green-500 hover:bg-green-400 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                <ArrowUp className="w-4 h-4 text-black" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
