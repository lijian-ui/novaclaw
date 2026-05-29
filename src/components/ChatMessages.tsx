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
  Clock, FileText, Settings, Wrench, ListTodo,
  ClipboardList, AlertTriangle, CheckCircle2,
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
  todo_write:   { icon: ListTodo, color: 'text-orange-400' },
  todo_list:    { icon: ListTodo, color: 'text-orange-400' },
  submit_plan:  { icon: ClipboardList, color: 'text-blue-400' },
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
              const jeevesIndex = fullPath.toLowerCase().indexOf('jeeves', appdataIndex)
              if (jeevesIndex !== -1) {
                displayPath = fullPath.slice(jeevesIndex + 6)
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
// 从工具结果中提取行变化信息（"+N -M" 格式，位于结果字符串开头）
function parseLineDiff(toolResult?: string): { added: number; removed: number } | null {
  if (!toolResult) return null
  const m = toolResult.match(/^\+(\d+)\s+-(\d+)/)
  if (!m) return null
  return { added: parseInt(m[1], 10), removed: parseInt(m[2], 10) }
}

// 显示格式：调用工具: [工具名称]：[工具参数] [+N -M]（一行显示）
function ToolCallBlock({
  toolName,
  argsJson,
  toolResult,
  isDone,
}: {
  toolName?: string
  argsJson: string
  toolResult?: string
  isDone?: boolean
}) {
  const { icon: Icon, color } = getToolMeta(toolName)
  const paramStr = extractToolParams(toolName, argsJson)
  const lineDiff = (toolName === 'write_file' || toolName === 'edit_file') && isDone ? parseLineDiff(toolResult) : null

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
        {lineDiff && (
          <span className="text-green-500/70 font-mono shrink-0 ml-1">
            {lineDiff.added > 0 && <span className="text-green-500">+{lineDiff.added}</span>}
            {lineDiff.added > 0 && lineDiff.removed > 0 && <span className="text-foreground/30"> </span>}
            {lineDiff.removed > 0 && <span className="text-red-500">-{lineDiff.removed}</span>}
          </span>
        )}
        {!isDone && (
          <span className="ml-auto shrink-0">
            <span className="inline-block w-3 h-3 rounded-full border-2 border-blue-400/30 border-t-blue-400 animate-spin" />
          </span>
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
    const text = Array.isArray(children) ? children.join('') : String(children)
    doCopy(text)
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
          <span className="text-[11px]">复制</span>
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

// ─── TerminalBlock ───────────────────────────────────────────────
// 终端风格的工具执行展示组件，用于 execute_command 工具
function TerminalBlock({
  toolName,
  argsJson,
  output,
  isExecuting,
  isError,
}: {
  toolName?: string
  argsJson: string
  output?: string
  isExecuting?: boolean
  isError?: boolean
}) {
  const [expanded, setExpanded] = useState(isExecuting)
  const scrollRef = useRef<HTMLDivElement>(null)
  const autoCollapsedRef = useRef(false)
  const { isDark } = useTheme()
  const { icon: Icon, color } = getToolMeta(toolName)
  const commandStr = extractToolParams(toolName, argsJson)
  const outputLines = output ? output.split('\n').length : 0

  // 执行中自动展开
  useEffect(() => {
    if (isExecuting) {
      setExpanded(true)
      autoCollapsedRef.current = false // 重置折叠标记
    }
  }, [isExecuting])

  // 执行完成后自动折叠（延迟 2 秒让用户看到结果）
  useEffect(() => {
    if (!isExecuting && output && !autoCollapsedRef.current) {
      autoCollapsedRef.current = true
      const timer = setTimeout(() => setExpanded(false), 2000)
      return () => clearTimeout(timer)
    }
  }, [isExecuting, output])

  // 自动滚动到最底部
  useEffect(() => {
    if (expanded && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [output, expanded])

  return (
    <div className={`my-2 rounded-lg border overflow-hidden ${
      isError
        ? 'border-red-500/30'
        : 'border-green-500/20'
    }`} style={{ background: isDark ? '#0d1117' : '#f8f9fa' }}>
      {/* 终端头部 - 点击折叠/展开 */}
      <button
        onClick={() => setExpanded(v => !v)}
        className={`w-full flex items-center gap-2 px-4 py-2.5 text-xs transition-colors ${
          isDark ? 'hover:bg-white/[0.03]' : 'hover:bg-black/[0.03]'
        }`}
      >
        {expanded
          ? <ChevronDown className={`w-3.5 h-3.5 shrink-0 ${isDark ? 'text-green-500/50' : 'text-green-600/60'}`} />
          : <ChevronRight className={`w-3.5 h-3.5 shrink-0 ${isDark ? 'text-green-500/50' : 'text-green-600/60'}`} />
        }
        <Icon className={`w-4 h-4 shrink-0 ${color}`} />
        <span className="text-blue-500/60 font-medium">执行命令:</span>
        <span className={`font-semibold font-mono truncate ${isDark ? 'text-green-500' : 'text-green-700'}`}>{commandStr}</span>

        {/* 执行状态指示 */}
        {isExecuting && (
          <span className="ml-auto flex items-center gap-1 text-[10px] text-amber-400/70">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
            执行中...
          </span>
        )}
        {isError && (
          <span className="ml-auto text-[10px] text-red-400/70">执行出错</span>
        )}
        {!isExecuting && !isError && output && (
          <span className={`ml-auto text-[10px] ${isDark ? 'text-green-500/50' : 'text-green-600/60'}`}>{outputLines} 行</span>
        )}
      </button>

      {/* 终端输出内容 */}
      <div
        className={`border-t transition-all duration-200 overflow-hidden ${
          isDark ? 'border-green-500/10' : 'border-green-600/15'
        }`}
        style={{
          maxHeight: expanded ? '600px' : '0px',
          opacity: expanded ? 1 : 0,
        }}
      >
        <div
          ref={scrollRef}
          className="p-3 text-xs leading-relaxed font-mono whitespace-pre-wrap overflow-y-auto"
          style={{ maxHeight: '600px', background: isDark ? '#0d1117' : '#f8f9fa', color: isDark ? '#22c55e' : '#166534' }}
        >
          {/* 命令提示符 */}
          <div className={`flex items-center gap-2 mb-2 select-none ${isDark ? 'text-green-500/60' : 'text-green-700/60'}`}>
            <span>{'>'}</span>
            <span className={isDark ? 'text-green-400/90' : 'text-green-700/90'}>{commandStr}</span>
          </div>

          {/* 输出内容 */}
          {output ? (
            <div className={`${isError ? (isDark ? 'text-red-400/80' : 'text-red-600/80') : (isDark ? 'text-green-400/70' : 'text-green-700/80')}`}>
              {output}
            </div>
          ) : !isExecuting ? (
            <div className={isDark ? 'text-green-500/30' : 'text-green-700/40'}>（命令已结束）</div>
          ) : null}
        </div>
      </div>
    </div>
  )
}

// ─── 复制按钮组件 ────────────────────────────────────────────────────
function MessageCopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(() => {
    doCopy(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }, [text])

  return (
    <button
      onClick={handleCopy}
      className="opacity-60 hover:opacity-100 transition-opacity p-1 rounded hover:bg-foreground/10"
      title="复制"
    >
      {copied ? <Check className="w-4 h-4 text-green-500" /> : <Copy className="w-4 h-4 text-foreground/50" />}
    </button>
  )
}

/** 复制文本到剪贴板，兼容 Tauri webview 和浏览器 */
function doCopy(text: string) {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).catch(() => fallbackCopy(text))
  } else {
    fallbackCopy(text)
  }
}

/** 降级方案：通过隐藏 textarea 执行复制 */
function fallbackCopy(text: string) {
  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.style.position = 'fixed'
  textarea.style.left = '-9999px'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  document.body.removeChild(textarea)
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
  inputTokens?: number
  outputTokens?: number
  cachedTokens?: number
  lastInputTokens?: number
  lastOutputTokens?: number
  /** 缓存命中率（0.0 ~ 1.0） */
  cacheHitRate?: number
  /** 流式消息中的临时 base64 图片 */
  images?: string[]
  /** 历史消息中的图片文件路径引用 */
  imagePaths?: string[]
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
    const toolName = msg.agentStep.toolName

    // execute_command 和 terminal 工具使用终端风格渲染
    if (toolName === 'execute_command' || toolName === 'terminal') {
      const isExecuting = st === 'tool_call' || st === 'function_call'
      const isError = st === 'tool_error'
      return (
        <div key={msg.id}>
          <TerminalBlock
            toolName={toolName}
            argsJson={msg.agentStep.content}
            output={msg.agentStep.toolResult}
            isExecuting={isExecuting}
            isError={isError}
          />
        </div>
      )
    }

    // todo_write / todo_list 使用任务清单卡片
    if (toolName === 'todo_write' || toolName === 'todo_list') {
      const isDone = st === 'tool_call_done' || st === 'tool_error'
      return (
        <div key={msg.id}>
          <TodoCard
            argsJson={msg.agentStep.content}
            result={msg.agentStep.toolResult}
            isDone={isDone}
          />
        </div>
      )
    }

    // submit_plan 使用计划卡片
    if (toolName === 'submit_plan') {
      return (
        <div key={msg.id}>
          <PlanCard
            argsJson={msg.agentStep.content}
            result={msg.agentStep.toolResult}
          />
        </div>
      )
    }

    // 其他工具使用简单的一行显示
    const isDone = st === 'tool_call_done' || st === 'tool_error'
    return (
      <div key={msg.id}>
        <ToolCallBlock
          toolName={toolName}
          argsJson={msg.agentStep.content}
          toolResult={msg.agentStep.toolResult}
          isDone={isDone}
        />
      </div>
    )
  }
  
  // tool_result / function_result：如果是 execute_command 则跳过（已在 tool_call 中展示）
  if (st === 'tool_result' || st === 'function_result') {
    if (msg.agentStep.toolName === 'execute_command' || msg.agentStep.toolName === 'terminal') {
      return null
    }
  }
  
  return null
}

// ─── TodoCard ────────────────────────────────────────────────────
// 用于 todo_write / todo_list 工具的美观卡片渲染
function TodoCard({
  argsJson,
  result,
  isDone,
}: {
  argsJson: string
  result?: string
  isDone: boolean
}) {
  // 解析 items 列表
  let items: { content: string; status: string; priority?: string }[] | null = null
  try {
    const args = JSON.parse(argsJson)
    if (Array.isArray(args.items)) {
      items = args.items
    }
  } catch {}

  const total = items?.length ?? 0
  const doneCount = items?.filter(i => i.status === 'completed').length ?? 0
  const progress = total > 0 ? Math.round((doneCount / total) * 100) : 0

  return (
    <div className="my-2 rounded-lg border border-orange-500/20 bg-orange-500/[0.03] overflow-hidden">
      {/* 头部 */}
      <div className="flex items-center gap-2 px-4 py-2.5 text-xs border-b border-orange-500/10">
        <ListTodo className="w-4 h-4 text-orange-400 shrink-0" />
        <span className="text-blue-500/60 font-medium">任务清单</span>
        {items && (
          <span className="ml-auto flex items-center gap-2">
            <span className="text-foreground/40">{doneCount}/{total}</span>
            <div className="w-20 h-1.5 rounded-full bg-foreground/10 overflow-hidden">
              <div
                className="h-full rounded-full bg-orange-400 transition-all duration-500"
                style={{ width: `${progress}%` }}
              />
            </div>
          </span>
        )}
      </div>

      {/* 列表内容 */}
      {items && items.length > 0 && (
        <div className="px-4 py-2 space-y-1">
          {items.map((item, i) => {
            const statusIcon = item.status === 'completed' ? '✅' : item.status === 'in_progress' ? '🔄' : '⬜'
            const priorityTag = item.priority === 'high'
              ? <span className="ml-1.5 px-1 py-0.5 rounded text-[10px] bg-red-500/10 text-red-400 border border-red-500/20">高</span>
              : item.priority === 'low'
                ? <span className="ml-1.5 px-1 py-0.5 rounded text-[10px] bg-foreground/5 text-foreground/40 border border-foreground/10">低</span>
                : null
            const isActive = item.status === 'in_progress'

            return (
              <div
                key={i}
                className={`flex items-center gap-2 px-2 py-1.5 rounded text-xs transition-colors ${
                  isActive ? 'bg-orange-500/10 border border-orange-500/20' : ''
                }`}
              >
                <span>{statusIcon}</span>
                <span className={`${item.status === 'completed' ? 'line-through text-foreground/40' : 'text-foreground/70'}`}>
                  {item.content}
                </span>
                {priorityTag}
                {isActive && <span className="ml-auto w-1.5 h-1.5 rounded-full bg-orange-400 animate-pulse" />}
              </div>
            )
          })}
        </div>
      )}

      {/* 结果文本（折叠显示） */}
      {isDone && result && (
        <details className="px-4 pb-2">
          <summary className="text-[10px] text-foreground/30 cursor-pointer hover:text-foreground/50">
            显示完整内容
          </summary>
          <pre className="mt-1 text-[10px] text-foreground/50 whitespace-pre-wrap font-mono">{result}</pre>
        </details>
      )}
    </div>
  )
}

// ─── PlanCard ─────────────────────────────────────────────────────
// 用于 submit_plan 工具的美观卡片渲染
function PlanCard({
  argsJson,
  result,
}: {
  argsJson: string
  result?: string
}) {
  let goal = ''
  let steps: { title: string; description: string; risk?: string }[] = []
  let summary = ''
  try {
    const args = JSON.parse(argsJson)
    goal = args.goal || ''
    if (Array.isArray(args.steps)) steps = args.steps
    summary = args.summary || ''
  } catch {}

  const highRiskCount = steps.filter(s => s.risk === 'high').length

  return (
    <div className="my-2 rounded-lg border border-blue-500/20 bg-blue-500/[0.03] overflow-hidden">
      {/* 头部 */}
      <div className="flex items-center gap-2 px-4 py-2.5 text-xs border-b border-blue-500/10">
        <ClipboardList className="w-4 h-4 text-blue-400 shrink-0" />
        <span className="text-blue-500/60 font-medium">执行计划</span>
        {highRiskCount > 0 && (
          <span className="ml-auto flex items-center gap-1 text-[10px] text-red-400/70">
            <AlertTriangle className="w-3 h-3" />
            {highRiskCount} 项高风险
          </span>
        )}
      </div>

      <div className="px-4 py-3 space-y-2">
        {/* 目标 */}
        {goal && (
          <div className="text-xs font-medium text-foreground/80 pb-2 border-b border-foreground/5">
            🎯 {goal}
          </div>
        )}

        {/* 步骤列表 */}
        {steps.map((step, i) => {
          const riskBadge = step.risk === 'high'
            ? <span className="px-1.5 py-0.5 rounded text-[10px] bg-red-500/10 text-red-400 border border-red-500/20">高风险</span>
            : step.risk === 'med'
              ? <span className="px-1.5 py-0.5 rounded text-[10px] bg-yellow-500/10 text-yellow-400 border border-yellow-500/20">中风险</span>
              : null

          return (
            <div key={i} className="flex items-start gap-2">
              <span className="w-5 h-5 rounded-full bg-blue-500/10 text-blue-400 text-[10px] flex items-center justify-center shrink-0 mt-0.5 font-medium">
                {i + 1}
              </span>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-xs font-medium text-foreground/70">{step.title}</span>
                  {riskBadge}
                </div>
                {step.description && (
                  <p className="text-[11px] text-foreground/40 mt-0.5">{step.description}</p>
                )}
              </div>
            </div>
          )
        })}

        {/* 总结 */}
        {summary && (
          <div className="text-[11px] text-foreground/50 pt-2 border-t border-foreground/5 italic">
            {summary}
          </div>
        )}

        {/* 计划状态提示 */}
        {result && result.includes('等待审批') && (
          <div className="flex items-center gap-2 px-3 py-2 rounded bg-blue-500/10 border border-blue-500/20 text-xs text-blue-400/80">
            <Clock className="w-3.5 h-3.5 shrink-0" />
            等待用户确认…
          </div>
        )}
        {result && result.includes('已批准') && (
          <div className="flex items-center gap-2 px-3 py-2 rounded bg-green-500/10 border border-green-500/20 text-xs text-green-400/80">
            <CheckCircle2 className="w-3.5 h-3.5 shrink-0" />
            计划已批准
          </div>
        )}
      </div>
    </div>
  )
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
    <div className="px-4 py-4 space-y-8">
      {/* 渲染所有历史消息（包括思考、工具调用、最终回复） */}
      {messages.map(msg => {
        // 用户消息
        if (msg.role === 'user') {
          return (
            <div key={msg.id} className="flex justify-end">
              <div className="max-w-[85%] rounded-xl px-4 py-3 pb-2 bg-green-500/15 border border-green-500/20">
                {/* 图片显示：base64（流式）或 imagePaths（历史） */}
                {((msg.images && msg.images.length > 0) || (msg.imagePaths && msg.imagePaths.length > 0)) && (
                  <div className="flex gap-1.5 mb-2 flex-wrap">
                    {(msg.images ?? []).map((url, i) => (
                      <img key={`img-${i}`} src={url} className="max-w-[200px] max-h-[160px] rounded-lg object-cover border border-border/30" alt="" />
                    ))}
                    {(msg.imagePaths ?? []).map((imgPath, i) => (
                      <img
                        key={`hp-${i}`}
                        src={`/api/files/image/${(msg as any).sessionId || '_'}/${imgPath}`}
                        className="max-w-[200px] max-h-[160px] rounded-lg object-cover border border-border/30"
                        alt=""
                        onError={(e) => { e.currentTarget.style.display = 'none' }}
                      />
                    ))}
                  </div>
                )}
                <p className="text-sm text-foreground/80 whitespace-pre-wrap break-all">{msg.content}</p>
                <div className="flex items-center justify-end gap-1 mt-2">
                  <MessageCopyButton text={msg.content} />
                </div>
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
                <div className="w-full rounded-xl px-4 py-4 pb-2 bg-white/80 dark:bg-foreground/[0.06] border border-foreground/10 shadow-sm">
                  <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
                    {cleaned}
                  </ReactMarkdown>
                  <div className="flex items-center gap-1 mt-3 pt-2 border-t border-foreground/5">
                    <MessageCopyButton text={cleaned} />
                    {msg.inputTokens !== undefined && (
                      <span className="ml-auto text-[10px] text-foreground/60 font-mono whitespace-nowrap">
                        {msg.lastInputTokens !== undefined && msg.lastInputTokens > 0 ? `本次：输入 ${msg.lastInputTokens} / 输出 ${msg.lastOutputTokens ?? 0} / ` : ''}累计：输入 {msg.inputTokens} / 输出 {msg.outputTokens ?? 0}{msg.cachedTokens !== undefined && msg.cachedTokens > 0 && ` / 缓存 ${msg.cachedTokens}`}{msg.cacheHitRate !== undefined && msg.cacheHitRate > 0 && ` / 缓存命中率 ${(msg.cacheHitRate * 100).toFixed(1)}%`}
                      </span>
                    )}
                  </div>
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
