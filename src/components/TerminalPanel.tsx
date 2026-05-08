import { useState, useRef, useEffect, useCallback } from 'react'
import { X, Square, Trash2 } from 'lucide-react'
import { useTerminal } from '@/hooks/useTerminal'
import type { TerminalLine } from '@/types/terminal'

const MIN_HEIGHT = 160
const MAX_HEIGHT = 500
const DEFAULT_HEIGHT = 280

interface TerminalPanelProps {
  visible: boolean
  onClose: () => void
}

/** 渲染单行终端输出 */
function TerminalOutputLine({ line }: { line: TerminalLine }) {
  const colorClass =
    line.kind === 'error' ? 'text-red-400' :
    line.kind === 'system' ? 'text-foreground/40' :
    'text-green-300/90'
  return (
    <div className={`whitespace-pre-wrap font-mono text-xs leading-relaxed ${colorClass}`}>
      {line.text}
    </div>
  )
}

export function TerminalPanel({ visible, onClose }: TerminalPanelProps) {
  const {
    lines, connected, running, error,
    history, historyIndex,
    sendCommand, killProcess, clearOutput, setHistoryIndex,
  } = useTerminal()

  const [input, setInput] = useState('')
  const [height, setHeight] = useState(DEFAULT_HEIGHT)
  const [resizing, setResizing] = useState(false)

  const inputRef = useRef<HTMLInputElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const startYRef = useRef(0)
  const startHeightRef = useRef(0)

  // 自动滚动到底部
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [lines])

  // 自动聚焦
  useEffect(() => {
    if (visible && inputRef.current) {
      inputRef.current.focus()
    }
  }, [visible])

  /** 回车执行 */
  const handleSend = useCallback(() => {
    if (!input.trim() || !connected) return
    sendCommand(input)
    setInput('')
  }, [input, connected, sendCommand])

  /** 键盘事件：上下箭头切换历史，回车执行，Ctrl+C 终止 */
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      handleSend()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      const idx = historyIndex === -1 ? history.length - 1 : Math.max(0, historyIndex - 1)
      setHistoryIndex(idx)
      setInput(history[idx] || '')
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      if (historyIndex === -1) return
      if (historyIndex >= history.length - 1) {
        setHistoryIndex(-1)
        setInput('')
      } else {
        const idx = historyIndex + 1
        setHistoryIndex(idx)
        setInput(history[idx] || '')
      }
    } else if ((e.ctrlKey || e.metaKey) && (e.key === 'c' || e.key === 'C')) {
      e.preventDefault()
      killProcess()
    } else if ((e.ctrlKey || e.metaKey) && (e.key === 'l' || e.key === 'L')) {
      e.preventDefault()
      clearOutput()
    }
  }, [handleSend, history, historyIndex, setHistoryIndex, killProcess, clearOutput])

  // 调整大小拖拽
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setResizing(true)
    startYRef.current = e.clientY
    startHeightRef.current = height
    document.body.style.cursor = 'row-resize'
    document.body.style.userSelect = 'none'
  }, [height])

  useEffect(() => {
    if (!resizing) return
    const handleMouseMove = (e: MouseEvent) => {
      const diff = startYRef.current - e.clientY
      const newHeight = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, startHeightRef.current + diff))
      setHeight(newHeight)
    }
    const handleMouseUp = () => {
      setResizing(false)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
    }
  }, [resizing])

  if (!visible) return null

  return (
    <div
      ref={panelRef}
      className="shrink-0 border-t border-border flex flex-col"
      style={{ height, background: '#0d1117' /* 黑色终端背景 */ }}
    >
      {/* 拖拽手柄 */}
      <div
        className="h-1.5 cursor-row-resize hover:bg-foreground/10 active:bg-foreground/20 transition-colors shrink-0 relative"
        onMouseDown={handleMouseDown}
      />

      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5 shrink-0">
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${connected ? 'bg-green-400' : 'bg-red-400'}`} />
          <span className="text-xs text-foreground/60 font-medium">Terminal</span>
          {running && (
            <span className="text-[10px] text-amber-400/70 animate-pulse">运行中...</span>
          )}
          {error && (
            <span className="text-[10px] text-red-400/70">{error}</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {/* 终止进程 */}
          <button
            title="终止进程 (Ctrl+C)"
            onClick={killProcess}
            disabled={!running}
            className="p-1 rounded hover:bg-foreground/10 transition-colors disabled:opacity-30"
          >
            <Square className="w-3.5 h-3.5 text-foreground/40" />
          </button>
          {/* 清屏 */}
          <button
            title="清屏 (Ctrl+L)"
            onClick={clearOutput}
            className="p-1 rounded hover:bg-foreground/10 transition-colors"
          >
            <Trash2 className="w-3.5 h-3.5 text-foreground/40" />
          </button>
          {/* 关闭 */}
          <button
            title="关闭终端"
            onClick={onClose}
            className="p-1 rounded hover:bg-foreground/10 transition-colors"
          >
            <X className="w-3.5 h-3.5 text-foreground/40" />
          </button>
        </div>
      </div>

      {/* 输出区域 */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto px-3 py-1.5 font-mono text-xs leading-relaxed"
        style={{ background: '#0d1117' }}
      >
        {lines.length === 0 ? (
          <div className="text-foreground/20 text-center pt-8">
            <p>输入命令开始</p>
            <p className="text-[10px] mt-1">方向键↑↓切换历史 · Ctrl+C 终止 · Ctrl+L 清屏</p>
          </div>
        ) : (
          lines.map((line, i) => <TerminalOutputLine key={i} line={line} />)
        )}
      </div>

      {/* 输入区域 */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-t border-foreground/10 shrink-0">
        <span className="text-xs text-emerald-400 font-mono shrink-0">$</span>
        <input
          ref={inputRef}
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          className="flex-1 bg-transparent text-xs text-foreground/80 font-mono outline-none"
          placeholder={connected ? (running ? '进程运行中，输入将加入下一轮...' : '输入命令...') : '连接中...'}
          disabled={!connected}
        />
      </div>
    </div>
  )
}
