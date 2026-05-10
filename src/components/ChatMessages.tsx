import { useState, useCallback, useEffect, useRef } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import mermaid from 'mermaid'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark, vs } from 'react-syntax-highlighter/dist/esm/styles/prism'
import {
  Copy, Check, ChevronRight, ChevronDown, Brain, Circle, Search,
  Code2, Puzzle, Cpu, Blocks, Terminal, Clock, FileText, Settings,
} from 'lucide-react'
import { useTheme } from '@/contexts/ThemeContext'
import type { Components } from 'react-markdown'
import type { CSSProperties } from 'react'

// 初始化 Mermaid（theme 由 useEffect 动态设置）
mermaid.initialize({
  startOnLoad: false,
  securityLevel: 'loose',
  fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
})

// 深色主题代码高亮
const darkTheme: Record<string, CSSProperties> = {
  ...oneDark,
  'pre[class*="language-"]': {
    ...(oneDark['pre[class*="language-"]'] || {}),
    background: '#1e2329',
  },
  'code[class*="language-"]': {
    ...(oneDark['code[class*="language-"]'] || {}),
    background: '#1e2329',
  },
}

// 浅色主题代码高亮
const lightTheme: Record<string, CSSProperties> = {
  ...vs,
  'pre[class*="language-"]': {
    ...(vs['pre[class*="language-"]'] || {}),
    background: '#f3f3f3',
  },
  'code[class*="language-"]': {
    ...(vs['code[class*="language-"]'] || {}),
    background: '#f3f3f3',
  },
}

// ---- 思考过程显示开关 ----
// true  → 流式输出时在折叠块内实时显示思考内容
// false → 流式时隐藏内容，完成后仅显示可折叠的标题栏
const SHOW_THINK_STREAMING = true


function parseAllThinkBlocks(content: string): { thinkBlocks: string[]; answer: string } {
  const thinkBlocks: string[] = []
  let result = content

  // 处理完整的 <think>...</think> 标签
  let fullThinkRegex = /<think\s*>([\s\S]*?)<\/think\s*>/gi
  let match: RegExpExecArray | null
  while ((match = fullThinkRegex.exec(content)) !== null) {
    const thinkContent = match[1].trim()
    if (thinkContent) {
      thinkBlocks.push(thinkContent)
    }
  }
  // 从结果中完全移除所有完整的 <think> 块
  result = result.replace(/<think\s*>[\s\S]*?<\/think\s*>/gi, '').trim()

  // 处理 Google Gemma 风格的 <|channel>thought...<channel|>
  // 兼容 <|channel> 和 <|channel|> 两种开头写法
  if (content.includes('<|channel')) {
    const googleRegex = /<\|channel\|?>thought\s*([\s\S]*?)<channel\|>/gi
    while ((match = googleRegex.exec(content)) !== null) {
      const thinkContent = match[1].trim()
      if (thinkContent && !thinkBlocks.includes(thinkContent)) {
        thinkBlocks.push(thinkContent)
      }
    }
    result = result.replace(/<\|channel\|?>thought[\s\S]*?<channel\|>/gi, '').trim()
  }

  // 处理未闭合的 <think> 标签（流式状态）
  // 注意：不限制 thinkBlocks.length === 0，确保多轮思考时后续未闭合块也能被捕获
  const partialThink = result.match(/<think\s*>([\s\S]*)$/)
  if (partialThink && !result.includes('</think')) {
    const thinkContent = partialThink[1].trim()
    if (thinkContent) {
      thinkBlocks.push(thinkContent)
    }
    result = result.replace(/<think\s*>[\s\S]*$/gi, '').trim()
  }

  if (result.includes('<|channel')) {
    const partialGoogle = result.match(/<\|channel\|?>thought\s*([\s\S]*)$/)
    if (partialGoogle && !result.includes('<channel|>')) {
      const thinkContent = partialGoogle[1].trim()
      if (thinkContent && !thinkBlocks.includes(thinkContent)) {
        thinkBlocks.push(thinkContent)
      }
      result = result.replace(/<\|channel\|?>thought\s*[\s\S]*$/gi, '').trim()
    }
  }

  return { thinkBlocks, answer: result }
}

// ---- ThinkingBlock ----
function ThinkingBlock({ content, streaming, title, defaultExpanded, secondary, index }: {
  content: string
  streaming?: boolean
  title?: string
  defaultExpanded?: boolean
  secondary?: boolean
  index?: number
}) {
  const [expanded, setExpanded] = useState(!!defaultExpanded)
  const scrollRef = useRef<HTMLDivElement>(null)
  // 流式阶段是否展开内容由 SHOW_THINK_STREAMING 开关控制
  const showContent = streaming ? SHOW_THINK_STREAMING : expanded
  const prevStreamingRef = useRef(streaming)

  useEffect(() => {
    // 流式输出完成时（streaming: true → false），自动折叠
    if (prevStreamingRef.current && !streaming) {
      setExpanded(false)
    }
    prevStreamingRef.current = streaming
  }, [streaming])

  useEffect(() => {
    if (streaming && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [content, streaming])

  return (
    <div className={`my-2 rounded-lg border overflow-hidden group ${
      secondary
        ? 'border-foreground/[0.08] bg-foreground/[0.01]'
        : 'border-border bg-foreground/[0.02]'
    }`}>
      <button
        onClick={() => !streaming && setExpanded(!expanded)}
        className={`w-full flex items-center gap-2 px-3 py-2 text-xs transition-colors cursor-pointer ${
          secondary
            ? 'text-foreground/40 hover:text-foreground/60 hover:bg-foreground/[0.02]'
            : 'text-foreground/50 hover:text-foreground/70 hover:bg-foreground/[0.03]'
        }`}
      >
        {streaming ? (
          <>
            <Brain className="w-3.5 h-3.5 text-amber-500 animate-pulse shrink-0" />
            <span className={`font-medium ${secondary ? 'text-amber-500/60' : 'text-amber-500/80'}`}>
              {title || '正在思考...'}
            </span>
          </>
        ) : (
          <>
            <span className={`w-3.5 flex justify-center shrink-0 transition-opacity ${
              secondary ? 'opacity-40' : 'opacity-0 group-hover:opacity-100'
            }`}>
              {expanded ? (
                <ChevronDown className="w-3.5 h-3.5" />
              ) : (
                <ChevronRight className="w-3.5 h-3.5" />
              )}
            </span>
            {secondary ? (
              <Circle className="w-2.5 h-2.5 shrink-0 text-foreground/30" />
            ) : (
              <Brain className="w-3.5 h-3.5 shrink-0 text-amber-500" />
            )}
            <span className={`font-medium ${secondary ? 'text-foreground/50' : ''}`}>
              {title || (secondary ? `Thought ${index || ''}` : '思考过程')}
            </span>
          </>
        )}
      </button>

      <div
        className={`border-t transition-all duration-300 ease-in-out overflow-hidden ${
          secondary ? 'border-foreground/[0.06]' : 'border-border'
        }`}
        style={{
          maxHeight: showContent ? '400px' : '0px',
          opacity: showContent ? 1 : 0,
        }}
      >
        <div
          ref={scrollRef}
          className={`px-4 py-3 text-xs leading-relaxed whitespace-pre-wrap overflow-y-auto ${
            secondary
              ? 'text-foreground/45 italic'
              : 'text-foreground/60 italic'
          }`}
          style={{ maxHeight: streaming ? '300px' : '400px' }}
        >
          {content}
        </div>
      </div>
    </div>
  )
}

// ---- 工具类型图标映射 ----
const toolIconMap: Record<string, { icon: React.ComponentType<{ className?: string }>; color: string }> = {
  editor:    { icon: Code2,    color: 'text-emerald-500' },
  skills:    { icon: Puzzle,   color: 'text-violet-500' },
  model:     { icon: Cpu,      color: 'text-blue-500' },
  agent:     { icon: Brain,    color: 'text-amber-500' },
  mcp:       { icon: Blocks,   color: 'text-cyan-500' },
  terminal:  { icon: Terminal, color: 'text-green-500' },
  schedule:  { icon: Clock,    color: 'text-orange-500' },
  logs:      { icon: FileText, color: 'text-foreground/50' },
  settings:  { icon: Settings, color: 'text-foreground/50' },
  read_file:    { icon: FileText, color: 'text-blue-400' },
  write_file:   { icon: Code2,    color: 'text-green-400' },
  edit_file:    { icon: Code2,    color: 'text-yellow-400' },
  glob:         { icon: FileText, color: 'text-purple-400' },
  grep:         { icon: Search,   color: 'text-orange-400' },
  web_search:   { icon: Search,   color: 'text-cyan-400' },
  memory:       { icon: Brain,    color: 'text-amber-400' },
}

function getToolMeta(toolName?: string) {
  if (!toolName) return { icon: Puzzle, color: 'text-violet-500' }
  const key = toolName.toLowerCase().replace(/[\s_-]/g, '')
  return toolIconMap[key] || { icon: Puzzle, color: 'text-violet-500' }
}

// ---- ToolCallBlock ----
function ToolCallBlock({
  toolName,
  content,
  toolResult,
}: {
  toolName?: string
  content: string
  toolResult?: string
}) {
  const { icon: Icon, color } = getToolMeta(toolName)
  const displayName = toolName || 'unknown'

  // 尝试从 content 中提取文件路径
  function extractFilePath(content: string): string | null {
    // JSON 格式: {"file_path": "/path/to/file.txt"}
    const jsonMatch = content.match(/"file_path"\s*:\s*"([^"]+)"/)
    if (jsonMatch) return jsonMatch[1]

    // JSON 格式: {"path": "/path/to/file.txt"}
    const jsonPathMatch = content.match(/"path"\s*:\s*"([^"]+)"/)
    if (jsonPathMatch) return jsonPathMatch[1]

    // JSON 格式: {"file": "/path/to/file.txt"}
    const jsonFileMatch = content.match(/"file"\s*:\s*"([^"]+)"/)
    if (jsonFileMatch) return jsonFileMatch[1]

    // JSON 格式: {"filepath": "/path/to/file.txt"}
    const jsonFilepathMatch = content.match(/"filepath"\s*:\s*"([^"]+)"/)
    if (jsonFilepathMatch) return jsonFilepathMatch[1]

    // 常见参数名: directory, folder, target, source 等
    const dirMatch = content.match(/"(directory|folder|target|source|dest|output|input)"\s*:\s*"([^"]+)"/i)
    if (dirMatch) return dirMatch[2]

    // 裸路径: /path/to/file 或 ./path/to/file 或 path/to/file
    const barePathMatch = content.match(/["']([./]?[a-zA-Z]:[/\\])?[\w.-]+[\w./\\-]+["']/)
    if (barePathMatch) {
      const path = barePathMatch[1].replace(/["']/g, '')
      // 过滤掉太短的路径（可能是参数名）
      if (path.length > 3 && (path.includes('/') || path.includes('\\') || path.includes('.'))) {
        return path
      }
    }

    return null
  }

  const filePath = extractFilePath(content)

  return (
    <div className="my-2 rounded-lg border border-border bg-foreground/[0.02] overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2">
        <Icon className={`w-3.5 h-3.5 shrink-0 ${color}`} />
        <span className="font-medium text-foreground/80 text-xs">{displayName}</span>
        {filePath && (
          <span className="text-xs text-foreground/50 font-mono truncate ml-1" title={filePath}>
            {filePath}
          </span>
        )}
      </div>
    </div>
  )
}

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

  return (
    <div className="relative group my-2 rounded-lg overflow-hidden border border-foreground/10">
      <div className="flex items-center justify-between px-3 py-1.5 bg-foreground/5 border-b border-foreground/5 text-[11px] text-foreground/40">
        <span>{lang}</span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-foreground/10 transition-colors"
        >
          {copied ? (
            <Check className="w-3 h-3 text-green-500" />
          ) : (
            <Copy className="w-3 h-3" />
          )}
          <span>{copied ? '已复制' : '复制'}</span>
        </button>
      </div>
      <SyntaxHighlighter
        style={isDark ? darkTheme : lightTheme}
        language={lang}
        PreTag="div"
        customStyle={{ 
          margin: 0, 
          borderRadius: 0, 
          fontSize: '12px',
          background: isDark ? '#1e2329' : '#f3f3f3',
        }}
      >
        {children}
      </SyntaxHighlighter>
    </div>
  )
}

function InlineCode({ children }: { children: React.ReactNode }) {
  return (
    <code className="px-1.5 py-0.5 rounded bg-foreground/10 text-[12px] text-emerald-500 font-mono">
      {children}
    </code>
  )
}

const markdownComponents: Components = {
  code({ className, children }: { className?: string; children?: React.ReactNode }) {
    const isInline = !className
    const content = String(children).replace(/\n$/, '')
    if (isInline) {
      return <InlineCode>{children}</InlineCode>
    }
    // Mermaid 图表：渲染为 <pre class="mermaid">，后续由 mermaid.run() 转换为 SVG
    const match = /language-(\w+)/.exec(className || '')
    if (match && match[1] === 'mermaid') {
      return <pre className="mermaid">{content}</pre>
    }
    return <CodeBlock className={className}>{content}</CodeBlock>
  },
  pre({ children }) {
    return <>{children}</>
  },
  p({ children, ...props }) {
    return <p className="text-sm text-foreground/80 leading-relaxed mb-2 last:mb-0" {...props}>{children}</p>
  },
  ul({ children, ...props }) {
    return <ul className="text-sm text-foreground/80 space-y-1 mb-2 pl-5 list-disc" {...props}>{children}</ul>
  },
  ol({ children, ...props }) {
    return <ol className="text-sm text-foreground/80 space-y-1 mb-2 pl-5 list-decimal" {...props}>{children}</ol>
  },
  li({ children, ...props }) {
    return <li className="leading-relaxed" {...props}>{children}</li>
  },
  h1({ children, ...props }) {
    return <h1 className="text-base font-bold text-foreground mb-2 mt-3" {...props}>{children}</h1>
  },
  h2({ children, ...props }) {
    return <h2 className="text-sm font-bold text-foreground mb-1.5 mt-2.5" {...props}>{children}</h2>
  },
  h3({ children, ...props }) {
    return <h3 className="text-sm font-semibold text-foreground/90 mb-1 mt-2" {...props}>{children}</h3>
  },
  blockquote({ children, ...props }) {
    return <blockquote className="border-l-2 border-foreground/20 pl-3 my-2 text-foreground/60 italic" {...props}>{children}</blockquote>
  },
  table({ children, ...props }) {
    return (
      <div className="overflow-x-auto my-2">
        <table className="w-full text-sm border-collapse" {...props}>{children}</table>
      </div>
    )
  },
  th({ children, ...props }) {
    return <th className="border border-foreground/10 px-3 py-1.5 text-left text-foreground/70 font-medium bg-foreground/5" {...props}>{children}</th>
  },
  td({ children, ...props }) {
    return <td className="border border-foreground/10 px-3 py-1.5 text-foreground/70" {...props}>{children}</td>
  },
  hr({ ...props }) {
    return <hr className="border-foreground/10 my-3" {...props} />
  },
  a({ children, href, ...props }) {
    return <a href={href} className="text-blue-500 hover:text-blue-400 underline" target="_blank" rel="noreferrer" {...props}>{children}</a>
  },
  strong({ children, ...props }) {
    return <strong className="font-semibold text-foreground" {...props}>{children}</strong>
  },
}

interface MarkdownContentProps {
  content: string
  streaming?: boolean
}

function MarkdownContent({ content, streaming }: MarkdownContentProps) {
  const { thinkBlocks, answer } = parseAllThinkBlocks(content)
  const thinkClosed = content.includes('</think') || content.includes('<channel|>')
  const thinkingStreaming = streaming && !thinkClosed

  return (
    <>
      {thinkBlocks.length > 0 && (
        <ThinkingBlock 
          content={thinkBlocks[0]} 
          streaming={thinkingStreaming}
          title={thinkingStreaming ? '正在思考...' : '思考过程'}
          defaultExpanded={!thinkingStreaming}
        />
      )}

      {thinkBlocks.slice(1).map((tb, i) => (
        <ThinkingBlock 
          key={`thought_${i}`}
          content={tb} 
          title="Thought"
          secondary
          defaultExpanded
          index={i + 2}
        />
      ))}

      {answer && (
        <div className="mt-2">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={markdownComponents}
          >
            {answer}
          </ReactMarkdown>
        </div>
      )}
    </>
  )
}

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
  messagesEndRef: React.Ref<HTMLDivElement>
}

export function ChatMessages({ messages, isStreaming, streamingContent, streamingReasoning, messagesEndRef }: ChatMessagesProps) {
  const { isDark } = useTheme()
  
  // 每次渲染后自动渲染 Mermaid 图表（根据当前明暗主题切换主题色）
  useEffect(() => {
    mermaid.initialize({
      startOnLoad: false,
      theme: isDark ? 'dark' : 'default',
      securityLevel: 'loose',
      fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
    })
    mermaid.run()
  }, [isDark])

  const mergedResultMap = new Map<string, string>()
  const toolStepIds = new Set<string>()
  const resultStepIds = new Set<string>()

  // 预处理1：合并工具调用和结果
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]
    if (msg.role !== 'agent_step' || !msg.agentStep) continue
    const st = msg.agentStep.stepType?.toLowerCase() || ''
    const next = messages[i + 1]
    if (
      (st === 'tool_call' || st === 'function_call') &&
      next?.role === 'agent_step' &&
      next?.agentStep &&
      next.agentStep.toolName?.toLowerCase() === msg.agentStep.toolName?.toLowerCase() &&
      (next.agentStep.stepType === 'tool_result' || next.agentStep.stepType === 'function_result')
    ) {
      const result = next.agentStep.toolResult || next.agentStep.content || ''
      mergedResultMap.set(msg.id, result)
      toolStepIds.add(msg.id)
      resultStepIds.add(next.id)
    }
  }

  // 预处理2：识别属于 assistant 的 thought 步骤（避免在外层循环重复渲染）
  const thoughtStepIds = new Set<string>()
  // 预处理3：识别 first_thought 步骤（独立渲染为"思考过程"）
  const firstThoughtStepIds = new Set<string>()
  for (let i = 0; i < messages.length; i++) {
    if (messages[i].role !== 'assistant') continue
    for (let j = i - 1; j >= 0; j--) {
      const m = messages[j]
      if (m.role === 'user' || m.role === 'assistant') break
      if (m.role === 'agent_step' && m.agentStep) {
        const st = m.agentStep.stepType?.toLowerCase() || ''
        // 区分 first_thought 和普通 thought
        if (st === 'first_thought') {
          firstThoughtStepIds.add(m.id)
        } else if ((st.includes('thought') || st.includes('think') || st === 'reasoning') && !toolStepIds.has(m.id)) {
          thoughtStepIds.add(m.id)
        }
      }
    }
  }

  // 预处理4：流式阶段收集尚未被任何 assistant 消息关联的 agent_step 消息
  // 这些步骤应该按照时间顺序渲染，而不是等到 assistant 出现
  const pendingAgentSteps: MessageData[] = []
  const pairedToolResultIds = new Set<string>()
  // 在 isStreaming 块外部初始化，块内部填充，用于后续跳过已渲染步骤
  const pendingRenderedIds = new Set<string>()
  if (isStreaming) {
    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i]
      if (msg.role !== 'agent_step') continue
      // 跳过已被 assistant 向后关联的步骤
      if (toolStepIds.has(msg.id)) continue
      if (thoughtStepIds.has(msg.id)) continue
      if (firstThoughtStepIds.has(msg.id)) continue
      pendingAgentSteps.push(msg)
      pendingRenderedIds.add(msg.id)
    }

    // 流式阶段配对 tool_call 和 tool_result
    for (let i = 0; i < pendingAgentSteps.length; i++) {
      const msg = pendingAgentSteps[i]
      if (msg.agentStep?.stepType === 'tool_result') {
        // 尝试在之前的 pendingAgentSteps 中找对应的 tool_call
        for (let j = 0; j < i; j++) {
          const prevMsg = pendingAgentSteps[j]
          if (
            (prevMsg.agentStep?.stepType === 'tool_call' || prevMsg.agentStep?.stepType === 'function_call') &&
            prevMsg.agentStep?.toolName?.toLowerCase() === msg.agentStep?.toolName?.toLowerCase()
          ) {
            pairedToolResultIds.add(msg.id)
            pendingRenderedIds.add(msg.id) // 已配对的 tool_result 也标记为已渲染
            break
          }
        }
      }
    }

    // 将 pendingAgentSteps 中的步骤标记为"已被渲染"，避免后续 assistant 扫描时重复收集
    for (const pendingMsg of pendingAgentSteps) {
      const st = pendingMsg.agentStep?.stepType?.toLowerCase() || ''
      if (st === 'first_thought') {
        firstThoughtStepIds.add(pendingMsg.id)
      } else if (st.includes('thought') || st.includes('think') || st === 'reasoning') {
        thoughtStepIds.add(pendingMsg.id)
      } else if (st === 'tool_call' || st === 'function_call' || st === 'tool_result' || st === 'function_result') {
        toolStepIds.add(pendingMsg.id)
      }
    }
  }

  const renderList: { key: string; element: JSX.Element }[] = []

  // 创建 pendingAgentSteps 的 Map，方便快速查找
  const pendingAgentStepsMap = new Map<string, MessageData>()
  for (const msg of pendingAgentSteps) {
    pendingAgentStepsMap.set(msg.id, msg)
  }

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]

    if (resultStepIds.has(msg.id)) continue

    // 如果是 pendingAgentSteps 中的消息，在这里渲染（保持时间顺序）
    if (pendingAgentStepsMap.has(msg.id)) {
      const pendingMsg = pendingAgentStepsMap.get(msg.id)!
      const st = pendingMsg.agentStep?.stepType?.toLowerCase() || ''
      const isFirstThought = st === 'first_thought'
      const isThought = st.includes('thought') || st.includes('think') || st === 'reasoning'

      if (isFirstThought) {
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <ThinkingBlock
                  content={pendingMsg.agentStep!.content}
                  streaming={isStreaming}
                  title={isStreaming ? '正在思考...' : '思考过程'}
                  defaultExpanded={false}
                />
              </div>
            </div>
          ),
        })
      } else if (isThought) {
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <ThinkingBlock
                  content={pendingMsg.agentStep!.content}
                  streaming={isStreaming}
                  title={isStreaming ? '正在思考...' : 'Thought'}
                  secondary
                  defaultExpanded={false}
                />
              </div>
            </div>
          ),
        })
      } else if (st === 'tool_call' || st === 'function_call') {
        // 查找对应的 tool_result
        let toolResult: string | undefined
        for (const [id, nextMsg] of pendingAgentStepsMap) {
          if (
            (nextMsg.agentStep?.stepType === 'tool_result' || nextMsg.agentStep?.stepType === 'function_result') &&
            nextMsg.agentStep?.toolName?.toLowerCase() === pendingMsg.agentStep?.toolName?.toLowerCase() &&
            pairedToolResultIds.has(id)
          ) {
            toolResult = nextMsg.agentStep?.toolResult || nextMsg.agentStep?.content
            break
          }
        }
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <ToolCallBlock
                  toolName={pendingMsg.agentStep!.toolName}
                  content={pendingMsg.agentStep!.content}
                  toolResult={toolResult}
                />
              </div>
            </div>
          ),
        })
      } else if (st === 'tool_result' || st === 'function_result') {
        // 已配对的 tool_result 不单独渲染（已合并到上面的 tool_call 中）
        if (pairedToolResultIds.has(msg.id)) continue
        // 未配对的 tool_result 单独渲染
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <div className="rounded-xl px-4 py-2 bg-foreground/[0.04] border border-foreground/5">
                  <p className="text-xs text-foreground/60 whitespace-pre-wrap">{pendingMsg.agentStep!.content}</p>
                </div>
              </div>
            </div>
          ),
        })
      } else {
        // 其他类型的步骤
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <div className="rounded-xl px-4 py-2 bg-foreground/[0.04] border border-foreground/5">
                  <p className="text-xs text-foreground/60 whitespace-pre-wrap">{pendingMsg.agentStep!.content}</p>
                </div>
              </div>
            </div>
          ),
        })
      }
      continue // 跳过后续处理，因为已经在这里渲染了
    }

    // 跳过流式阶段已渲染的 agent_step
    if (pendingRenderedIds.has(msg.id)) continue

    if (msg.role === 'assistant') {
      const assistantContent = msg.content
      const { thinkBlocks, answer } = parseAllThinkBlocks(assistantContent)

      // 向前查找，按原始顺序收集当前 assistant 之前的所有 agent_step
      // 保留每个步骤的类型信息，以便按真实顺序渲染
      type AgentStepEntry =
        | { kind: 'tool'; msg: MessageData }
        | { kind: 'thought'; msg: MessageData }
        | { kind: 'first_thought'; msg: MessageData }

      const agentSteps: AgentStepEntry[] = []
      for (let j = i - 1; j >= 0; j--) {
        const m = messages[j]
        if (m.role === 'user' || m.role === 'assistant') break
        if (m.role !== 'agent_step' || !m.agentStep) continue

        if (toolStepIds.has(m.id)) {
          agentSteps.unshift({ kind: 'tool', msg: m })
        } else {
          const st = m.agentStep.stepType?.toLowerCase() || ''
          if (st === 'first_thought') {
            agentSteps.unshift({ kind: 'first_thought', msg: m })
          } else if (st.includes('thought') || st.includes('think') || st === 'reasoning') {
            agentSteps.unshift({ kind: 'thought', msg: m })
          }
        }
      }

      // ── 渲染顺序 ──────────────────────────────────────────────
      // 1. 第一个 <think> 块 → "思考过程" 折叠块，完成后自动折叠
      if (thinkBlocks.length > 0) {
        renderList.push({
          key: `${msg.id}_first_think`,
          element: (
            <div className="flex justify-start" key={`${msg.id}_first_think`}>
              <div className="w-full min-w-0">
                <ThinkingBlock
                  content={thinkBlocks[0]}
                  title="思考过程"
                  defaultExpanded={false}
                />
              </div>
            </div>
          ),
        })
      }

      // 2 & 3. agent_step 步骤按原始顺序渲染：
      //   first_thought → "思考过程" 折叠块（主要样式）
      //   thought → "Thought" 折叠块（次要样式，完成后自动折叠）
      //   tool    → ToolCallBlock
      for (const entry of agentSteps) {
        if (entry.kind === 'first_thought') {
          // 第一次思考：显示为"思考过程"（主要样式）
          renderList.push({
            key: entry.msg.id,
            element: (
              <div className="flex justify-start" key={entry.msg.id}>
                <div className="w-full min-w-0">
                  <ThinkingBlock
                    content={entry.msg.agentStep!.content}
                    title="思考过程"
                    defaultExpanded={false}
                  />
                </div>
              </div>
            ),
          })
        } else if (entry.kind === 'thought') {
          renderList.push({
            key: entry.msg.id,
            element: (
              <div className="flex justify-start" key={entry.msg.id}>
                <div className="w-full min-w-0">
                  <ThinkingBlock
                    content={entry.msg.agentStep!.content}
                    title="Thought"
                    secondary
                    defaultExpanded={false}
                  />
                </div>
              </div>
            ),
          })
        } else {
          renderList.push({
            key: entry.msg.id,
            element: (
              <div className="flex justify-start" key={entry.msg.id}>
                <div className="w-full min-w-0">
                  <ToolCallBlock
                    toolName={entry.msg.agentStep!.toolName}
                    content={entry.msg.agentStep!.content}
                    toolResult={mergedResultMap.get(entry.msg.id) || ''}
                  />
                </div>
              </div>
            ),
          })
        }
      }

      // 4. 后续 <think> 块（thinkBlocks[1..]） → "Thought" 折叠块，完成后自动折叠
      if (thinkBlocks.length > 1) {
        thinkBlocks.slice(1).forEach((tb, idx) => {
          renderList.push({
            key: `${msg.id}_thought_${idx}`,
            element: (
              <div className="flex justify-start" key={`${msg.id}_thought_${idx}`}>
                <div className="w-full min-w-0">
                  <ThinkingBlock
                    content={tb}
                    title="Thought"
                    secondary
                    defaultExpanded={false}
                    index={idx + 2}
                  />
                </div>
              </div>
            ),
          })
        })
      }

      // 5. 最终答案 → Markdown 渲染
      if (answer.trim()) {
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full rounded-xl px-4 py-3 bg-foreground/[0.04] border border-foreground/5">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  components={markdownComponents}
                >
                  {answer}
                </ReactMarkdown>
              </div>
            </div>
          ),
        })
      }
    } else if (msg.role === 'user') {
      renderList.push({
        key: msg.id,
        element: (
          <div className="flex justify-end" key={msg.id}>
            <div className="max-w-[85%] rounded-xl px-4 py-3 bg-green-500/15 border border-green-500/20">
              <p className="text-sm text-foreground/80 whitespace-pre-wrap">{msg.content}</p>
            </div>
          </div>
        ),
      })
    } else if (msg.role === 'agent_step' && msg.agentStep) {
      // 跳过已合并到 assistant 中的步骤（tool_call / tool_result）
      if (toolStepIds.has(msg.id)) continue
      // 跳过已被 assistant 消息向前扫描收录的 thought 步骤
      if (thoughtStepIds.has(msg.id)) continue
      // 跳过已被 assistant 消息向前扫描收录的 first_thought 步骤
      if (firstThoughtStepIds.has(msg.id)) continue
      // 跳过流式阶段已渲染的步骤
      if (pendingRenderedIds.has(msg.id)) continue

      const st = msg.agentStep.stepType?.toLowerCase() || ''
      const isFirstThought = st === 'first_thought'
      const isThought = st.includes('thought') || st.includes('think') || st === 'reasoning'

      // 流式阶段：尚未配对到任何 assistant 消息的独立 agent_step
      // thought 步骤正确传入 streaming=true，使其显示为"正在思考..."动画
      if (isFirstThought) {
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <ThinkingBlock
                  content={msg.agentStep.content}
                  streaming={isStreaming}
                  title={isStreaming ? '正在思考...' : '思考过程'}
                  defaultExpanded={false}
                />
              </div>
            </div>
          ),
        })
      } else if (isThought) {
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <ThinkingBlock
                  content={msg.agentStep.content}
                  streaming={isStreaming}
                  title={isStreaming ? '正在思考...' : 'Thought'}
                  secondary
                  defaultExpanded={false}
                />
              </div>
            </div>
          ),
        })
      } else {
        renderList.push({
          key: msg.id,
          element: (
            <div className="flex justify-start" key={msg.id}>
              <div className="w-full min-w-0">
                <div className="rounded-xl px-4 py-2 bg-foreground/[0.04] border border-foreground/5">
                  <p className="text-xs text-foreground/60 whitespace-pre-wrap">{msg.agentStep.content}</p>
                </div>
              </div>
            </div>
          ),
        })
      }
    }
  }

  return (
    <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
      {renderList.map(item => item.element)}
      {/* Streaming reasoning 块 */}
      {isStreaming && streamingReasoning && (
        <div className="flex justify-start">
          <div className="w-full rounded-xl px-4 py-3 bg-amber-500/5 border border-amber-500/20">
            <div className="flex items-center gap-2 mb-2">
              <div className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
              <span className="text-[11px] font-medium text-amber-400/70 uppercase tracking-wider">思考过程</span>
            </div>
            <MarkdownContent content={streamingReasoning} streaming={true} />
          </div>
        </div>
      )}
      {/* Streaming assistant text */}
      {isStreaming && streamingContent && (
        <div className="flex justify-start">
          <div className="w-full rounded-xl px-4 py-3 bg-foreground/[0.04] border border-foreground/5">
            <MarkdownContent content={streamingContent} streaming={true} />
            <span className="inline-block w-2 h-4 bg-foreground/60 animate-pulse ml-0.5" />
          </div>
        </div>
      )}
      <div ref={messagesEndRef} />
    </div>
  )
}
