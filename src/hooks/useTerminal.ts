import { useState, useCallback, useRef, useEffect } from 'react'
import type { Terminal } from '@xterm/xterm'
import type { TerminalMessage, UseTerminalReturn } from '@/types/terminal'

/** 终端 WebSocket URL 生成 */
function getTerminalWsUrl(): string {
  if (typeof window === 'undefined') {
    return 'ws://127.0.0.1:3000/ws/terminal'
  }
  if (import.meta.env.DEV) {
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    return `${proto}//${window.location.host}/ws/terminal`
  }
  return 'ws://127.0.0.1:3000/ws/terminal'
}

const isTauri = (): boolean =>
  typeof window !== 'undefined' && !!(window as any).__TAURI__?.invoke

export function useTerminal(
  terminalRef: React.MutableRefObject<Terminal | null>,
): UseTerminalReturn {
  const [connected, setConnected] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const runningRef = useRef(false)
  const [, forceUpdate] = useState(0)

  /** 处理后端消息 */
  const handleMessage = useCallback(
    (msg: TerminalMessage) => {
      const term = terminalRef.current
      switch (msg.type) {
        case 'stdout':
          term?.write(msg.data || '')
          break
        case 'stderr':
          term?.write(msg.data || '')
          break
        case 'exit':
          runningRef.current = false
          term?.write(`\r\n\x1b[33m[Process exited] code: ${msg.code ?? '?'}\x1b[0m\r\n`)
          forceUpdate((n) => n + 1)
          break
        case 'error':
          term?.write(`\r\n\x1b[31m[Error] ${msg.data}\x1b[0m\r\n`)
          runningRef.current = false
          forceUpdate((n) => n + 1)
          break
        case 'session_restarted':
          term?.write(`\r\n\x1b[32m[Session restarted]\x1b[0m\r\n`)
          break
      }
    },
    [terminalRef],
  )

  /** 建立 WebSocket 连接 */
  const connect = useCallback(() => {
    const existing = wsRef.current
    if (existing && (existing.readyState === WebSocket.OPEN || existing.readyState === WebSocket.CONNECTING)) {
      return
    }
    if (existing) {
      existing.close()
    }

    if (isTauri()) {
      setConnected(true)
      setError(null)
      const { listen, invoke } = (window as any).__TAURI__
      if (listen) {
        const unlisteners: Array<() => void> = []
        listen('terminal:stdout', (event: any) => {
          terminalRef.current?.write(event.payload || '')
        }).then((fn: () => void) => unlisteners.push(fn))
        listen('terminal:stderr', (event: any) => {
          terminalRef.current?.write(event.payload || '')
        }).then((fn: () => void) => unlisteners.push(fn))
        listen('terminal:exit', (event: any) => {
          runningRef.current = false
          terminalRef.current?.write(`\r\n\x1b[33m[Process exited] code: ${event.payload ?? '?'}\x1b[0m\r\n`)
          forceUpdate((n) => n + 1)
        }).then((fn: () => void) => unlisteners.push(fn))
        listen('terminal:connected', () => {
          setConnected(true)
        }).then((fn: () => void) => unlisteners.push(fn))
      }
      invoke('terminal_spawn').catch((e: unknown) => {
        terminalRef.current?.write(`\r\n\x1b[31mFailed to start terminal: ${e}\x1b[0m\r\n`)
      })
      return
    }

    const wsUrl = getTerminalWsUrl()
    const ws = new WebSocket(wsUrl)
    wsRef.current = ws

    ws.onopen = () => {
      setConnected(true)
      setError(null)
    }

    ws.onmessage = (event) => {
      try {
        const msg: TerminalMessage = JSON.parse(event.data)
        handleMessage(msg)
      } catch {
        terminalRef.current?.write(event.data)
      }
    }

    ws.onclose = () => {
      if (wsRef.current === ws) {
        setConnected(false)
        runningRef.current = false
        wsRef.current = null
        forceUpdate((n) => n + 1)
      }
    }

    ws.onerror = () => {
      setError('WebSocket connection failed')
    }
  }, [handleMessage, terminalRef])

  useEffect(() => {
    connect()
    return () => {
      const ws = wsRef.current
      wsRef.current = null
      setConnected(false)
      runningRef.current = false
      if (ws) {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'kill' }))
          ws.close()
        } else if (ws.readyState === WebSocket.CONNECTING) {
          // 连接中时不能 send，等 open 后再关闭
          const onOpen = () => {
            ws.send(JSON.stringify({ type: 'kill' }))
            ws.close()
          }
          ws.addEventListener('open', onOpen, { once: true })
        }
      }
    }
  }, [connect])

  const disconnect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      try { wsRef.current.send(JSON.stringify({ type: 'kill' })) } catch {}
    }
    wsRef.current?.close()
    setConnected(false)
    runningRef.current = false
    forceUpdate((n) => n + 1)
  }, [])

  /** 发送输入（xterm onData 触发 → 写入 shell stdin） */
  const sendInput = useCallback((data: string) => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_write', { data }).catch(() => {})
      return
    }
    const ws = wsRef.current
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'stdin', data }))
    }
  }, [])

  /** 执行命令（向后兼容） */
  const sendCommand = useCallback((cmd: string) => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_exec', { command: cmd }).catch((e: unknown) => {
        terminalRef.current?.write(`\r\n\x1b[31mExecution failed: ${e}\x1b[0m\r\n`)
      })
      return
    }
    const ws = wsRef.current
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      if (ws?.readyState === WebSocket.CONNECTING) {
        ws.addEventListener('open', () => {
          ws.send(JSON.stringify({ type: 'exec', command: cmd, data: cmd }))
        }, { once: true })
      }
      return
    }
    ws.send(JSON.stringify({ type: 'exec', command: cmd, data: cmd }))
  }, [terminalRef])

  const killProcess = useCallback(() => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_kill').catch(() => {})
      return
    }
    wsRef.current?.send(JSON.stringify({ type: 'kill' }))
    runningRef.current = true
    forceUpdate((n) => n + 1)
  }, [])

  const clearOutput = useCallback(() => {
    terminalRef.current?.clear()
  }, [terminalRef])

  const resize = useCallback((cols: number, rows: number) => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_resize', { cols, rows }).catch(() => {})
      return
    }
    wsRef.current?.send(JSON.stringify({ type: 'resize', cols, rows }))
  }, [])

  const reconnect = useCallback(() => {
    setError(null)
    connect()
  }, [connect])

  return {
    connected,
    running: runningRef.current,
    error,
    sendInput,
    sendCommand,
    killProcess,
    clearOutput,
    resize,
    disconnect,
    reconnect,
  }
}
