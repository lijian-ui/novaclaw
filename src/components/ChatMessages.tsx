import { useState, useCallback, useEffect, useRef } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark, vs } from 'react-syntax-highlighter/dist/esm/styles/prism'
import {
  Copy, Check, ChevronRight, ChevronDown, Brain, Circle, Search,
  Code2, Puzzle, Cpu, Blocks, Terminal, Clock, FileText, Settings,
} from 'lucide-react'
import type { Components } from 'react-markdown'
import type { CSSProperties } from 'react'

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
  const [expandedArgs, setExpandedArgs] = useState(true)
  const [expandedResult, setExpandedResult] = useState(true)

  const { icon: Icon, color } = getToolMeta(toolName)
  const displayName = toolName || 'unknown'

  return (
    <div className="my-2 rounded-lg border border-border bg-foreground/[0.02] overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2">
        <Icon className={`w-3.5 h-3.5 shrink-0 ${color}`} />
        <span className="font-medium text-foreground/80 text-xs">{displayName}</span>
        {content && (
          <button
            onClick={() => setExpandedArgs(!expandedArgs)}
            className="ml-auto flex items-center gap-1 text-foreground/40 hover:text-foreground/60 transition-colors"
          >
            <ChevronDown
              className={`w-3 h-3 transition-transform duration-200 ${expandedArgs ? '' : '-rotate-90'}`}
            />
            <span className="text-xs">参数</span>
          </button>
        )}
      </div>

      {content && (
        <div
          className="transition-all duration-300 ease-in-out overflow-hidden"
          style={{
            maxHeight: expandedArgs ? '200px' : '0px',
            opacity: expandedArgs ? 1 : 0,
          }}
        >
          <div className="border-t border-border px-3 py-2">
            <div className="text-xs text-foreground/50 font-mono whitespace-pre-wrap leading-relaxed bg-foreground/[0.03] rounded px-2 py-1.5">
              {content || '(无参数)'}
            </div>
          </div>
        </div>
      )}

      {toolResult && (
        <div className="border-t border-border">
          <button
            onClick={() => setExpandedResult(!expandedResult)}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-foreground/[0.03] transition-colors"
          >
            <ChevronRight
              className={`w-3 h-3 text-foreground/40 transition-transform duration-200 ${expandedResult ? 'rotate-90' : ''}`}
            />
            <span className="text-xs text-foreground/50">执行结果</span>
          </button>
          <div
            className="transition-all duration-300 ease-in-out overflow-hidden"
            style={{
              maxHeight: expandedResult ? '300px' : '0px',
              opacity: expandedResult ? 1 : 0,
            }}
          >
            <div className="px-3 pb-2">
              <div className="text-xs text-foreground/60 font-mono whitespace-pre-wrap leading-relaxed bg-foreground/[0.03] rounded px-2 py-1.5 max-h-[280px] overflow-y-auto">
                {toolResult}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function CodeBlock({ className, children }: { className?: string; children: string }) {
  const [copied, setCopied] = useState(false)
  const match = /language-(\w+)/.exec(className || '')
  const lang = match ? match[1] : 'text'
  const isDark = document.documentElement.classList.contains('dark')

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
  messagesEndRef: React.Ref<HTMLDivElement>
}

export function ChatMessages({ messages, isStreaming, streamingContent, messagesEndRef }: ChatMessagesProps) {
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
  for (let i = 0; i < messages.length; i++) {
    if (messages[i].role !== 'assistant') continue
    for (let j = i - 1; j >= 0; j--) {
      const m = messages[j]
      if (m.role === 'user' || m.role === 'assistant') break
      if (m.role === 'agent_step' && m.agentStep) {
        const st = m.agentStep.stepType?.toLowerCase() || ''
        if ((st.includes('thought') || st.includes('think') || st === 'reasoning') && !toolStepIds.has(m.id)) {
          thoughtStepIds.add(m.id)
        }
      }
    }
  }

  const renderList: { key: string; element: JSX.Element }[] = []

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]

    if (resultStepIds.has(msg.id)) continue

    if (msg.role === 'assistant') {
      const assistantContent = msg.content
      const { thinkBlocks, answer } = parseAllThinkBlocks(assistantContent)

      // 向前查找，按原始顺序收集当前 assistant 之前的所有 agent_step
      // 保留每个步骤的类型信息，以便按真实顺序渲染
      type AgentStepEntry =
        | { kind: 'tool'; msg: MessageData }
        | { kind: 'thought'; msg: MessageData }

      const agentSteps: AgentStepEntry[] = []
      for (let j = i - 1; j >= 0; j--) {
        const m = messages[j]
        if (m.role === 'user' || m.role === 'assistant') break
        if (m.role !== 'agent_step' || !m.agentStep) continue

        if (toolStepIds.has(m.id)) {
          agentSteps.unshift({ kind: 'tool', msg: m })
        } else {
          const st = m.agentStep.stepType?.toLowerCase() || ''
          if (st.includes('thought') || st.includes('think') || st === 'reasoning') {
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
      //   thought → "Thought" 折叠块（次要样式，完成后自动折叠）
      //   tool    → ToolCallBlock
      for (const entry of agentSteps) {
        if (entry.kind === 'thought') {
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

      const st = msg.agentStep.stepType?.toLowerCase() || ''
      const isThought = st.includes('thought') || st.includes('think') || st === 'reasoning'

      // 流式阶段：尚未配对到任何 assistant 消息的独立 agent_step
      // thought 步骤正确传入 streaming=true，使其显示为"正在思考..."动画
      renderList.push({
        key: msg.id,
        element: (
          <div className="flex justify-start" key={msg.id}>
            <div className="w-full min-w-0">
              {isThought ? (
                <ThinkingBlock
                  content={msg.agentStep.content}
                  streaming={isStreaming}
                  title={isStreaming ? '正在思考...' : 'Thought'}
                  secondary
                  defaultExpanded={false}
                />
              ) : (
                <div className="rounded-xl px-4 py-2 bg-foreground/[0.04] border border-foreground/5">
                  <p className="text-xs text-foreground/60 whitespace-pre-wrap">{msg.agentStep.content}</p>
                </div>
              )}
            </div>
          </div>
        ),
      })
    }
  }

  return (
    <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
      {renderList.map(item => item.element)}
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
