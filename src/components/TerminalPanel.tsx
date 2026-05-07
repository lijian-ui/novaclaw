import { useState, useRef, useEffect, useCallback } from 'react'
import { X } from 'lucide-react'

const WS_TERMINAL = 'ws://127.0.0.1:3000/ws/terminal'
const MIN_HEIGHT = 160
const MAX_HEIGHT = 500
const DEFAULT_HEIGHT = 280

interface TerminalPanelProps {
  visible: boolean
  onClose: () => void
}

export function TerminalPanel({ visible, onClose }: TerminalPanelProps) {
  const [input, setInput] = useState('')
  const [output, setOutput] = useState<string[]>([])
  const [connected, setConnected] = useState(false)
  const [height, setHeight] = useState(DEFAULT_HEIGHT)
  const [resizing, setResizing] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const startYRef = useRef(0)
  const startHeightRef = useRef(0)

  // Connect WebSocket
  useEffect(() => {
    if (!visible) {
      if (wsRef.current) {
        wsRef.current.close()
        wsRef.current = null
      }
      return
    }

    const ws = new WebSocket(WS_TERMINAL)
    wsRef.current = ws

    ws.onopen = () => {
      setConnected(true)
      setOutput(prev => [...prev, '-- Terminal connected --'])
    }

    ws.onmessage = (event) => {
      setOutput(prev => [...prev, event.data])
    }

    ws.onclose = () => {
      setConnected(false)
      setOutput(prev => [...prev, '-- Terminal disconnected --'])
      wsRef.current = null
    }

    ws.onerror = () => {
      setOutput(prev => [...prev, '-- Terminal connection error --'])
    }

    return () => {
      ws.close()
    }
  }, [visible])

  // Auto scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [output])

  // Auto focus
  useEffect(() => {
    if (visible && inputRef.current) {
      inputRef.current.focus()
    }
  }, [visible])

  const handleSend = useCallback(() => {
    if (!input.trim() || !wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return
    wsRef.current.send(input + '\n')
    setInput('')
  }, [input])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      handleSend()
    }
  }, [handleSend])

  // Resize handlers
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
    <div ref={panelRef} className="shrink-0 border-t border-border bg-muted flex flex-col" style={{ height }}>
      {/* Resize handle */}
      <div
        className="h-1.5 cursor-row-resize hover:bg-foreground/10 active:bg-foreground/20 transition-colors shrink-0 relative"
        onMouseDown={handleMouseDown}
      />

      {/* Header */}
      <div className="flex items-center justify-between px-3 py-1.5 shrink-0">
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${connected ? 'bg-green-400' : 'bg-red-400'}`} />
          <span className="text-xs text-foreground/60 font-medium">Terminal</span>
        </div>
        <button onClick={onClose} className="p-1 rounded hover:bg-foreground/10 transition-colors">
          <X className="w-3.5 h-3.5 text-foreground/40" />
        </button>
      </div>

      {/* Output */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-1 font-mono text-xs leading-relaxed">
        {output.map((line, i) => (
          <div key={i} className="whitespace-pre-wrap text-foreground/80">{line}</div>
        ))}
      </div>

      {/* Input */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-t border-border shrink-0">
        <span className="text-xs text-green-400/80 font-mono shrink-0">$</span>
        <input
          ref={inputRef}
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          className="flex-1 bg-transparent text-xs text-foreground/80 font-mono outline-none"
          placeholder={connected ? '输入命令...' : '连接中...'}
          disabled={!connected}
        />
      </div>
    </div>
  )
}
