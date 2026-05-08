import { useState, useCallback, useRef, useEffect } from 'react'
import type { TerminalLine, UseTerminalReturn } from '@/types/terminal'

const WS_URL = 'ws://127.0.0.1:3000/ws/terminal'
const MAX_LINES = 2000
const MAX_HISTORY = 100

/** 检测 Tauri 环境 */
const isTauri = (): boolean =>
  typeof window !== 'undefined' && !!(window as any).__TAURI__?.invoke

export function useTerminal(): UseTerminalReturn {
  const [lines, setLines] = useState<TerminalLine[]>([])
  const [connected, setConnected] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [history, setHistory] = useState<string[]>([])
  const [historyIndex, setHistoryIndex] = useState(-1)

  const wsRef = useRef<WebSocket | null>(null)
  const runningRef = useRef(false)
  const [, forceUpdate] = useState(0)

  const addLine = useCallback((text: string, kind: TerminalLine['kind']) => {
    setLines(prev => {
      const next = [...prev, { text, kind, timestamp: Date.now() }]
      return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next
    })
  }, [])

  /** 处理 WebSocket 消息 */
  const handleMessage = useCallback((data: string) => {
    try {
      const msg = JSON.parse(data)
      switch (msg.type) {
        case 'stdout':
          addLine(msg.data, 'normal')
          break
        case 'stderr':
          addLine(msg.data, 'error')
          break
        case 'exit':
          runningRef.current = false
          forceUpdate(n => n + 1)
          break
        case 'clear':
          setLines([])
          break
        case 'error':
          addLine(`[错误] ${msg.data}`, 'error')
          runningRef.current = false
          forceUpdate(n => n + 1)
          break
      }
    } catch {
      addLine(data, 'normal')
    }
  }, [addLine])

  // 连接 WebSocket
  useEffect(() => {
    const ws = new WebSocket(WS_URL)
    wsRef.current = ws

    ws.onopen = () => {
      setConnected(true)
      setError(null)
      addLine('-- 终端已连接 --', 'system')
      // 检测并提示可用 Tauri 原生
      if (isTauri()) {
        addLine('-- 桌面环境 (Tauri) --', 'system')
      } else {
        addLine('-- 浏览器环境 (WebSocket) --', 'system')
      }
    }

    ws.onmessage = (event) => {
      handleMessage(event.data)
    }

    ws.onclose = () => {
      setConnected(false)
      addLine('-- 终端已断开 --', 'system')
      wsRef.current = null
    }

    ws.onerror = () => {
      setError('WebSocket 连接失败')
      addLine('-- 连接错误 --', 'error')
    }

    return () => {
      ws.close()
    }
  }, [addLine, handleMessage])

  /** 发送命令 */
  const sendCommand = useCallback((cmd: string) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return

    const trimmed = cmd.trim()
    if (!trimmed) return

    // 用 JSON 格式发送（支持 kill 等协议）
    wsRef.current.send(JSON.stringify({
      type: 'exec',
      command: trimmed,
    }))

    // 记录历史
    setHistory(prev => {
      if (prev[prev.length - 1] === trimmed) return prev
      const next = [...prev, trimmed]
      return next.length > MAX_HISTORY ? next.slice(-MAX_HISTORY) : next
    })
    setHistoryIndex(-1)
    runningRef.current = true
    forceUpdate(n => n + 1)
  }, [])

  /** 终止当前进程 */
  const killProcess = useCallback(() => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return
    wsRef.current.send(JSON.stringify({ type: 'kill' }))
    addLine('\n[正在终止进程...]', 'system')
  }, [addLine])

  /** 清屏 */
  const clearOutput = useCallback(() => {
    setLines([])
  }, [])

  /** 断开连接 */
  const disconnect = useCallback(() => {
    wsRef.current?.close()
  }, [])

  return {
    lines,
    connected,
    running: runningRef.current,
    error,
    history,
    historyIndex,
    sendCommand,
    killProcess,
    clearOutput,
    setHistoryIndex,
    disconnect,
  }
}
