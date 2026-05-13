/**
 * ChatMessages.tsx - 聊天消息渲染组件
 * 
 * 【文件功能概述】
 * 该组件负责渲染聊天界面中的所有消息内容，包括用户消息、助手回复、工具调用、思考过程等。
 * 它是聊天界面的核心展示层，处理消息的格式化、高亮和交互逻辑。
 * 
 * 【主要组件/模块说明】
 * 1. ThinkingBlock - 思考过程展示组件（可折叠展开）
 *    - 支持流式思考内容的实时渲染
 *    - 区分首次思考(first_thought)和后续思考(thought)的样式
 *    - 默认折叠状态，用户点击可展开查看详细内容
 * 
 * 2. ToolCallBlock - 工具调用展示组件
 *    - 显示工具名称、图标和参数信息
 *    - 支持文件操作工具的特殊路径显示处理
 * 
 * 3. CodeBlock - 代码高亮展示组件
 *    - 支持多种编程语言语法高亮
 *    - 支持 Mermaid 图表渲染
 *    - 提供一键复制功能
 * 
 * 【数据处理逻辑】
 * - 消息类型分类：user(用户消息)、assistant(助手回复)、agent_step(代理步骤)
 * - agent_step 进一步细分：first_thought、thought、tool_call、tool_result 等
 * - Markdown 内容渲染：支持 GFM(GitHub Flavored Markdown)
 * - 代码块处理：自动识别语言类型并应用对应语法高亮
 * 
 * 【与其他文件的关联关系】
 * - 接收来自 ChatPanel.tsx 的 messages 数组数据
 * - 使用 ThemeContext 获取当前主题配置(深色/浅色模式)
 * - 依赖 react-markdown、react-syntax-highlighter、mermaid 等第三方库
 * 
 * 【使用场景】
 * - 作为 ChatPanel 的消息展示区域
 * - 渲染历史对话消息（从会话存储加载）
 * - 渲染实时流式消息（WebSocket 推送）
 * 
 * 【关键特性】
 * - 支持 Markdown 富文本渲染（标题、列表、表格、代码块等）
 * - 代码语法高亮（支持多种语言）
 * - Mermaid 图表渲染支持
 * - 思考过程的可折叠展示
 * - 工具调用的可视化展示
 * - 响应式设计，适配不同屏幕尺寸
 * - 深色/浅色主题切换支持
 */

import { useState, useCallback, useEffect, useRef } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import mermaid from 'mermaid'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark, vs } from 'react-syntax-highlighter/dist/esm/styles/prism'
import {
  Copy, Check, ChevronRight, ChevronDown, Brain,
  Search, Code2, Puzzle, Cpu, Blocks, Terminal,
  Clock, FileText, Settings, Wrench,
} from 'lucide-react'
import { useTheme } from '@/contexts/ThemeContext'
import type { Components } from 'react-markdown'
import type { CSSProperties } from 'react'

mermaid.initialize({ startOnLoad: false, securityLevel: 'loose' })

// ─── 代码高亮主题 ───────────────────────────────────────────────
const darkTheme: Record<string, CSSProperties> = {
  ...oneDark,
  'pre[class*="language-"]': { ...(oneDark['pre[class*="language-"]'] || {}), background: '#1e2329' },
  'code[class*="language-"]': { ...(oneDark['code[class*="language-"]'] || {}), background: '#1e2329' },
}
const lightTheme: Record<string, CSSProperties> = {
  ...vs,
  'pre[class*="language-"]': { ...(vs['pre[class*="language-"]'] || {}), background: '#f3f3f3' },
  'code[class*="language-"]': { ...(vs['code[class*="language-"]'] || {}), background: '#f3f3f3' },
}

// ─── 工具图标映射 ────────────────────────────────────────────────
const toolIconMap: Record<string, { icon: React.ComponentType<{ className?: string }>; color: string }> = {
  read_file:    { icon: FileText, color: 'text-blue-400' },
  write_file:   { icon: Code2,    color: 'text-green-400' },
  edit_file:    { icon: Code2,    color: 'text-yellow-400' },
  glob:         { icon: FileText, color: 'text-purple-400' },
  grep:         { icon: Search,   color: 'text-orange-400' },
  web_search:   { icon: Search,   color: 'text-cyan-400' },
  memory:       { icon: Brain,    color: 'text-amber-400' },
  skills_list:  { icon: Puzzle,   color: 'text-violet-400' },
  skill_view:   { icon: Puzzle,   color: 'text-violet-400' },
  todo:         { icon: Clock,    color: 'text-orange-400' },
  terminal:     { icon: Terminal, color: 'text-green-400' },
  settings:     { icon: Settings, color: 'text-foreground/50' },
  agent:        { icon: Cpu,      color: 'text-blue-400' },
  mcp:          { icon: Blocks,   color: 'text-cyan-400' },
}

function getToolMeta(toolName?: string) {
  if (!toolName) return { icon: Wrench, color: 'text-foreground/50' }
  const key = toolName.toLowerCase().replace(/[\s_-]/g, '_')
  return toolIconMap[key] || { icon: Wrench, color: 'text-foreground/50' }
}

// ─── 从工具参数 JSON 中提取关键参数用于展示 ──────────────────────
function extractToolParams(_toolName: string | undefined, argsJson: string): string {
  try {
    const args = JSON.parse(argsJson)
    if (!args || typeof args !== 'object') return argsJson.slice(0, 80)

    // 优先展示路径类参数
    const pathKeys = ['path', 'file_path', 'filepath', 'file', 'directory', 'folder', 'target', 'source', 'rel_path', 'relative_path']
    for (const k of pathKeys) {
      if (args[k]) {
        const fullPath = String(args[k])
        let displayPath = fullPath
        
        // 如果是绝对路径，尝试转换为相对路径显示
        if (fullPath.startsWith('/') || fullPath.includes(':')) {
          // 查找 workspace 相关路径
          const workspaceIndex = fullPath.toLowerCase().indexOf('workspace')
          if (workspaceIndex !== -1) {
            displayPath = fullPath.slice(workspaceIndex + 9)
          } else {
            // 查找 appdata/local 路径
            const appdataIndex = fullPath.toLowerCase().indexOf('appdata')
            if (appdataIndex !== -1) {
              const novaclawIndex = fullPath.toLowerCase().indexOf('novaclaw', appdataIndex)
              if (novaclawIndex !== -1) {
                displayPath = fullPath.slice(novaclawIndex + 8)
              }
            }
          }
        }
        
        // 确保路径不为空
        if (displayPath.startsWith('/') || displayPath.startsWith('\\')) {
          displayPath = displayPath.slice(1)
        }
        
        // 如果还是绝对路径，只显示文件名
        if (displayPath.startsWith('/') || displayPath.includes(':') || displayPath.startsWith('\\')) {
          const parts = displayPath.split(/[\\/]/)
          if (parts.length > 0) {
            displayPath = parts[parts.length - 1]
          }
        }
        
        return displayPath || fullPath
      }
    }
    
    // 搜索类
    if (args.query) return `"${String(args.query).slice(0, 60)}"`
    if (args.pattern) return args.pattern
    
    // 通用：取第一个值
    const firstVal = Object.values(args)[0]
    if (firstVal !== undefined) return String(firstVal).slice(0, 80)
  } catch {
    // 非 JSON，直接截取
    return argsJson.slice(0, 80)
  }
  return ''
}

// ─── ThinkingBlock 组件 ───────────────────────────────────────────────
// 功能：渲染思考过程的可折叠/展开块
// 参数：
//   content: 思考内容文本
//   streaming: 是否处于流式输出状态（模型正在思考）
//   isFirst: 是否是首次思考（影响样式，首次思考用琥珀色主题）
//   showStatus: 是否显示状态提示（如"模型开始思考"、"模型再次思考"）
function ThinkingBlock({
  content,
  streaming,
  isFirst,
  showStatus = true,
}: {
  content: string
  streaming?: boolean
  isFirst?: boolean
  showStatus?: boolean
}) {
  // expanded: 默认折叠状态，流式思考时自动展开
  const [expanded, setExpanded] = useState(streaming || false)
  
  // scrollRef: 内容容器的引用，用于自动滚动
  const scrollRef = useRef<HTMLDivElement>(null)
  
  // 监听 streaming 变化，流式开始时自动展开，流式结束后保持当前状态
  useEffect(() => {
    if (streaming) {
      setExpanded(true)
    }
  }, [streaming])

  // ─── 流式时自动滚动到底部 ────────────────────────────────────────
  useEffect(() => {
    if (streaming && expanded && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [content, streaming, expanded])

  // ─── 核心显示控制逻辑 ────────────────────────────────────────────
  // 流式时自动展开，非流式时默认折叠，用户可点击切换
  const showContent = expanded

  return (
    <div className={`my-1.5 rounded-lg border overflow-hidden transition-all duration-300 ${
      isFirst
        ? 'border-amber-500/20 bg-amber-500/[0.03]'
        : 'border-foreground/[0.07] bg-foreground/[0.01]'
    }`}>
     
      {/* ─── 折叠/展开按钮 ─────────────────────────────────────────── */}
          <button
            onClick={() => {
              setExpanded(v => !v)
            }}
            className="w-full flex items-center gap-2 px-3 py-2 text-xs transition-colors hover:bg-foreground/5"
          >
        {/* 方向箭头 */}
        {showContent
          ? <ChevronDown className="w-3.5 h-3.5 text-foreground/40 shrink-0" />
          : <ChevronRight className="w-3.5 h-3.5 text-foreground/40 shrink-0" />
        }
        
        {/* 大脑图标 */}
        <Brain className={`w-3.5 h-3.5 shrink-0 ${
          streaming 
            ? 'text-amber-500 animate-pulse' 
            : isFirst ? 'text-amber-500' : 'text-foreground/30'
        }`} />
        
        {/* 状态提示标签 */}
        {showStatus && (
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium leading-tight ${
            isFirst
              ? 'bg-amber-500/10 text-amber-500/80 border border-amber-500/20'
              : 'bg-blue-500/10 text-blue-500/80 border border-blue-500/20'
          }`}>
            {isFirst ? '模型开始思考' : '模型再次思考'}
          </span>
        )}
        
        {/* 标题 */}
        <span className={`font-medium ${
          streaming 
            ? 'text-amber-500/80' 
            : isFirst ? 'text-foreground/60' : 'text-foreground/40'
        }`}>
          {streaming ? '思考中...' : isFirst ? '思考过程' : 'Thought'}
        </span>
        
        {/* 流式状态指示 */}
        {streaming && content.length > 0 && (
          <span className="inline-block w-1.5 h-3 bg-amber-500/60 animate-pulse ml-0.5" />
        )}
        
        {/* 字符数提示 */}
        {content.length > 0 && !streaming && (
          <span className="text-foreground/20 text-[10px]">
            ({content.length > 500 ? '500+' : content.length} 字)
          </span>
        )}
      </button>

      {/* ─── 内容显示区域 ───────────────────────────────────────────── */}
      <div
        className={`border-t transition-all duration-200 overflow-hidden ${
          isFirst ? 'border-amber-500/10' : 'border-foreground/[0.05]'
        }`}
        style={{ 
          maxHeight: showContent ? '400px' : '0px',
          opacity: showContent ? 1 : 0,
          transition: 'max-height 0.2s ease-out, opacity 0.2s ease-out'
        }}
      >
        <div
          ref={scrollRef}
          className="px-4 py-3 text-xs leading-relaxed whitespace-pre-wrap overflow-y-auto text-foreground/55 italic"
          style={{ maxHeight: '400px' }}
        >
          {content}
        </div>
      </div>
    </div>
  )
}

// ─── ToolCallBlock ───────────────────────────────────────────────
// 工具调用显示组件
// 显示格式：调用工具: [工具名称]：[工具参数]（一行显示）
function ToolCallBlock({
  toolName,
  argsJson,
}: {
  toolName?: string
  argsJson: string
}) {
  const { icon: Icon, color } = getToolMeta(toolName)
  const paramStr = extractToolParams(toolName, argsJson)

  return (
    <div className="my-2 rounded-lg border border-blue-500/20 bg-blue-500/[0.03] transition-all duration-300">
      {/* 工具调用头部 - 一行显示 */}
      <div className="flex items-center gap-2 px-4 py-2.5 text-xs whitespace-nowrap overflow-hidden text-ellipsis">
        {/* 工具图标 */}
        <Icon className={`w-4 h-4 shrink-0 ${color}`} />
        <span className="text-blue-500/60 font-medium">调用工具:</span>
        <span className="font-semibold text-blue-600">{toolName || 'tool'}</span>
        <span className="text-foreground/40">：</span>
        {paramStr && (
          <span className="text-foreground/60 font-mono truncate">{paramStr}</span>
        )}
      </div>
    </div>
  )
}

// ─── CodeBlock ───────────────────────────────────────────────────
function CodeBlock({ className, children }: { className?: string; children: string }) {
  const { isDark } = useTheme()
  const [copied, setCopied] = useState(false)
  const match = /language-(\w+)/.exec(className || '')
  const lang = match ? match[1] : 'text'

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(children)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }, [children])

  if (lang === 'mermaid') {
    return <pre className="mermaid">{children}</pre>
  }

  return (
    <div className="relative my-2 rounded-lg overflow-hidden border border-foreground/10">
      <div className="flex items-center justify-between px-3 py-1.5 bg-foreground/5 border-b border-foreground/5 text-[11px] text-foreground/40">
        <span>{lang}</span>
        <button onClick={handleCopy} className="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-foreground/10 transition-colors">
          {copied ? <Check className="w-3 h-3 text-green-500" /> : <Copy className="w-3 h-3" />}
          <span>{copied ? '已复制' : '复制'}</span>
        </button>
      </div>
      <SyntaxHighlighter
        style={isDark ? darkTheme : lightTheme}
        language={lang}
        PreTag="div"
        customStyle={{ margin: 0, borderRadius: 0, fontSize: '12px', background: isDark ? '#1e2329' : '#f3f3f3' }}
      >
        {children}
      </SyntaxHighlighter>
    </div>
  )
}

const markdownComponents: Components = {
  code({ className, children }: { className?: string; children?: React.ReactNode }) {
    const content = String(children).replace(/\n$/, '')
    if (!className) {
      return <code className="px-1.5 py-0.5 rounded bg-foreground/10 text-[12px] text-emerald-500 font-mono">{children}</code>
    }
    return <CodeBlock className={className}>{content}</CodeBlock>
  },
  pre({ children }) { return <>{children}</> },
  p({ children, ...props }) { return <p className="text-sm text-foreground/80 leading-relaxed mb-2 last:mb-0" {...props}>{children}</p> },
  ul({ children, ...props }) { return <ul className="text-sm text-foreground/80 space-y-1 mb-2 pl-5 list-disc" {...props}>{children}</ul> },
  ol({ children, ...props }) { return <ol className="text-sm text-foreground/80 space-y-1 mb-2 pl-5 list-decimal" {...props}>{children}</ol> },
  li({ children, ...props }) { return <li className="leading-relaxed" {...props}>{children}</li> },
  h1({ children, ...props }) { return <h1 className="text-base font-bold text-foreground mb-2 mt-3" {...props}>{children}</h1> },
  h2({ children, ...props }) { return <h2 className="text-sm font-bold text-foreground mb-1.5 mt-2.5" {...props}>{children}</h2> },
  h3({ children, ...props }) { return <h3 className="text-sm font-semibold text-foreground/90 mb-1 mt-2" {...props}>{children}</h3> },
  blockquote({ children, ...props }) { return <blockquote className="border-l-2 border-foreground/20 pl-3 my-2 text-foreground/60 italic" {...props}>{children}</blockquote> },
  table({ children, ...props }) { return <div className="overflow-x-auto my-2"><table className="w-full text-sm border-collapse" {...props}>{children}</table></div> },
  th({ children, ...props }) { return <th className="border border-foreground/10 px-3 py-1.5 text-left text-foreground/70 font-medium bg-foreground/5" {...props}>{children}</th> },
  td({ children, ...props }) { return <td className="border border-foreground/10 px-3 py-1.5 text-foreground/70" {...props}>{children}</td> },
  hr({ ...props }) { return <hr className="border-foreground/10 my-3" {...props} /> },
  a({ children, href, ...props }) { return <a href={href} className="text-blue-500 hover:text-blue-400 underline" target="_blank" rel="noreferrer" {...props}>{children}</a> },
  strong({ children, ...props }) { return <strong className="font-semibold text-foreground" {...props}>{children}</strong> },
}

// ─── 类型定义 ────────────────────────────────────────────────────
export interface AgentStepInfo {
  stepType: string
  content: string
  toolName?: string
  toolResult?: string
  turn: number
  maxTurns: number
}

export interface MessageData {
  id: string
  role: 'user' | 'assistant' | 'agent_step'
  content: string
  agentStep?: AgentStepInfo
}

interface ChatMessagesProps {
  messages: MessageData[]
  isStreaming: boolean
  streamingContent: string
  streamingReasoning?: string
  isRethinking?: boolean
  messagesEndRef: React.Ref<HTMLDivElement>
}

// ─── 从 content 中剥离 <think> 标签 ─────────────────────────────
function stripThinkTags(content: string): string {
  return content
    .replace(/<think\s*>[\s\S]*?<\/think\s*>/gi, '')
    .replace(/<think\s*>[\s\S]*$/i, '')
    .trim()
}

// ─── 渲染单个 agent_step（支持流式和历史消息）─────────────────────
function renderAgentStep(msg: MessageData, _isStreaming: boolean): JSX.Element | null {
  if (msg.role !== 'agent_step' || !msg.agentStep) return null
  
  const st = msg.agentStep.stepType.toLowerCase()
  
  if (st === 'first_thought' || st === 'reasoning' || st === 'thought') {
    return (
      <div key={msg.id}>
        <ThinkingBlock 
          content={msg.agentStep.content} 
          streaming={false}
          isFirst={st === 'first_thought' || st === 'reasoning'}
        />
      </div>
    )
  }
  
  if (st === 'tool_call' || st === 'function_call' || st === 'tool_call_done' || st === 'tool_error') {
    return (
      <div key={msg.id}>
        <ToolCallBlock
          toolName={msg.agentStep.toolName}
          argsJson={msg.agentStep.content}
        />
      </div>
    )
  }
  
  if (st === 'tool_result' || st === 'function_result') {
    return null
  }
  
  return null
}

// ─── 主组件 ──────────────────────────────────────────────────────
export function ChatMessages({
  messages,
  isStreaming,
  streamingContent,
  streamingReasoning,
  isRethinking,
  messagesEndRef,
}: ChatMessagesProps) {
  const { isDark } = useTheme()

  useEffect(() => {
    mermaid.initialize({ startOnLoad: false, theme: isDark ? 'dark' : 'default', securityLevel: 'loose' })
    mermaid.run()
  }, [isDark])

  return (
    <div className="px-4 py-4 space-y-3">
      {/* 渲染所有历史消息（包括思考、工具调用、最终回复） */}
      {messages.map(msg => {
        // 用户消息
        if (msg.role === 'user') {
          return (
            <div key={msg.id} className="flex justify-end">
              <div className="max-w-[85%] rounded-xl px-4 py-3 bg-green-500/15 border border-green-500/20">
                <p className="text-sm text-foreground/80 whitespace-pre-wrap">{msg.content}</p>
              </div>
            </div>
          )
        }
        
        // agent_step（思考或工具调用）
        const agentStepEl = renderAgentStep(msg, isStreaming)
        if (agentStepEl) return agentStepEl
        
        // assistant 最终回复
        if (msg.role === 'assistant' && msg.content.trim()) {
          const cleaned = stripThinkTags(msg.content)
          if (cleaned) {
            return (
              <div key={msg.id} className="flex justify-start animate-fade-in">
                <div className="w-full rounded-xl px-4 py-4 bg-white/80 dark:bg-foreground/[0.06] border border-foreground/10 shadow-sm">
                  <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
                    {cleaned}
                  </ReactMarkdown>
                </div>
              </div>
            )
          }
        }
        
        return null
      })}

      {/* 流式思考内容（在流式最终回复之前） */}
      {isStreaming && streamingReasoning && (
        <div key="streaming-reasoning">
          <ThinkingBlock 
            content={streamingReasoning} 
            streaming={true}
            isFirst={!isRethinking}
          />
        </div>
      )}

      {/* 流式阶段的最终回复（边思考边输出） */}
      {isStreaming && streamingContent && (
        <div className="flex justify-start animate-fade-in">
          <div className="w-full rounded-xl px-4 py-4 bg-white/80 dark:bg-foreground/[0.06] border border-foreground/10 shadow-sm">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
              {stripThinkTags(streamingContent)}
            </ReactMarkdown>
            <span className="inline-block w-1 h-4 bg-foreground/50 animate-pulse ml-0.5 align-middle" />
          </div>
        </div>
      )}

      {/* 无内容时显示等待动画 */}
      {isStreaming && !streamingContent && !streamingReasoning && messages.length === 0 && (
        <div className="flex items-center gap-2 px-1 py-1">
          <span className="w-1.5 h-1.5 rounded-full bg-foreground/30 animate-bounce" style={{ animationDelay: '0ms' }} />
          <span className="w-1.5 h-1.5 rounded-full bg-foreground/30 animate-bounce" style={{ animationDelay: '150ms' }} />
          <span className="w-1.5 h-1.5 rounded-full bg-foreground/30 animate-bounce" style={{ animationDelay: '300ms' }} />
        </div>
      )}

      <div ref={messagesEndRef} />
    </div>
  )
}
