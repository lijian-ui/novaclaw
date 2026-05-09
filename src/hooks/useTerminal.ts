/**
 * useTerminal — 终端钩子（持久化 Shell 模式）
 *
 * 管理 WebSocket 连接，将 Shell 进程的 stdout/stderr 输出实时写入 xterm.js。
 * 用户通过 xterm.onData 发送输入，经由 WebSocket 的 stdin 消息写入 Shell 进程。
 * 启动时自动建立连接，无需手动点击"重连"。
 */

import { useState, useCallback, useRef, useEffect } from 'react'
import type { Terminal } from '@xterm/xterm'
import type { TerminalMessage, UseTerminalReturn } from '@/types/terminal'

const WS_URL = 'ws://127.0.0.1:3000/ws/terminal'

/** 检测 Tauri 环境 */
const isTauri = (): boolean =>
  typeof window !== 'undefined' && !!(window as any).__TAURI__?.invoke

/** 将 stderr 包装为 ANSI 红色（xterm.js 自动渲染） */
function wrapStderr(data: string): string {
  return `\x1b[31m${data}\x1b[0m`
}

export function useTerminal(
  terminalRef: React.MutableRefObject<Terminal | null>,
): UseTerminalReturn {
  const [connected, setConnected] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const runningRef = useRef(false)
  const [, forceUpdate] = useState(0)

  /** 写入终端 */
  const write = useCallback(
    (data: string) => {
      terminalRef.current?.write(data)
    },
    [terminalRef],
  )

  /** 处理后端消息 */
  const handleMessage = useCallback(
    (msg: TerminalMessage) => {
      const term = terminalRef.current
      switch (msg.type) {
        case 'stdout':
          term?.write(msg.data || '')
          break
        case 'stderr':
          term?.write(wrapStderr(msg.data || ''))
          break
        case 'exit':
          runningRef.current = false
          term?.write(`\r\n\x1b[33m[进程退出] 代码: ${msg.code ?? '?'}\x1b[0m\r\n`)
          forceUpdate((n) => n + 1)
          break
        case 'clear':
          term?.clear()
          break
        case 'error':
          term?.write(`\r\n\x1b[31m[错误] ${msg.data}\x1b[0m\r\n`)
          runningRef.current = false
          forceUpdate((n) => n + 1)
          break
      }
    },
    [terminalRef],
  )

  // ---- 建立连接 ----
  const connect = useCallback(() => {
    // 关闭旧连接
    wsRef.current?.close()

    // Shell 进程一启动就是"运行中"状态（持久化）
    runningRef.current = true
    forceUpdate((n) => n + 1)

    if (isTauri()) {
      // Tauri 模式：监听 Tauri 事件
      setConnected(true)
      setError(null)
      
      const { listen, invoke } = (window as any).__TAURI__
      if (listen) {
        const unlisteners: Array<() => void> = []
        
        listen('terminal:stdout', (event: any) => {
          terminalRef.current?.write(event.payload || '')
        }).then((unlisten: () => void) => unlisteners.push(unlisten))

        listen('terminal:stderr', (event: any) => {
          terminalRef.current?.write(wrapStderr(event.payload || ''))
        }).then((unlisten: () => void) => unlisteners.push(unlisten))

        listen('terminal:exit', (event: any) => {
          runningRef.current = false
          terminalRef.current?.write(`\r\n\x1b[33m[进程退出] 代码: ${event.payload ?? '?'}\x1b[0m\r\n`)
          forceUpdate((n) => n + 1)
        }).then((unlisten: () => void) => unlisteners.push(unlisten))

        listen('terminal:clear', () => {
          terminalRef.current?.clear()
        }).then((unlisten: () => void) => unlisteners.push(unlisten))

        listen('terminal:connected', () => {
          setConnected(true)
        }).then((unlisten: () => void) => unlisteners.push(unlisten))
      }
      
      // 自动启动终端进程
      invoke('terminal_spawn').catch((e: unknown) => {
        terminalRef.current?.write(`\r\n\x1b[31m启动终端失败: ${e}\x1b[0m\r\n`)
      })
      return
    }

    // WebSocket 模式
    const ws = new WebSocket(WS_URL)
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
      setConnected(false)
      runningRef.current = false
      forceUpdate((n) => n + 1)
      wsRef.current = null
    }

    ws.onerror = () => {
      setError('WebSocket 连接失败')
    }
  }, [handleMessage, terminalRef])

  // ---- 自动连接（仅挂载时一次） ----
  useEffect(() => {
    connect()
    return () => {
      wsRef.current?.close()
      wsRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // ---- 断开 ----
  const disconnect = useCallback(() => {
    wsRef.current?.close()
    setConnected(false)
    runningRef.current = false
    forceUpdate((n) => n + 1)
  }, [])

  // ---- 发送输入（xterm onData → 后端 Shell stdin） ----
  const sendInput = useCallback((data: string) => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_write', { data }).catch(() => {})
    } else {
      const ws = wsRef.current
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'stdin', data }))
      }
    }
  }, [])

  // ---- 执行命令（向后兼容：将命令写入 stdin + 换行） ----
  const sendCommand = useCallback((cmd: string) => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_exec', { command: cmd }).catch((e: unknown) => {
        terminalRef.current?.write(`\r\n\x1b[31m执行失败: ${e}\x1b[0m\r\n`)
      })
    } else {
      const ws = wsRef.current
      if (!ws || ws.readyState !== WebSocket.OPEN) return
      ws.send(JSON.stringify({ type: 'exec', command: cmd }))
    }
  }, [terminalRef])

  // ---- 终止 ----
  const killProcess = useCallback(() => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_kill').catch(() => {})
    } else {
      wsRef.current?.send(JSON.stringify({ type: 'kill' }))
    }
    runningRef.current = true // 后续连接会重新 spawn shell
    forceUpdate((n) => n + 1)
  }, [])

  // ---- 清屏 ----
  const clearOutput = useCallback(() => {
    terminalRef.current?.clear()
  }, [terminalRef])

  // ---- 尺寸调整 ----
  const resize = useCallback((_cols: number, _rows: number) => {
    if (isTauri()) {
      const { invoke } = (window as any).__TAURI__
      invoke('terminal_resize', { cols: _cols, rows: _rows }).catch(() => {})
    } else {
      wsRef.current?.send(
        JSON.stringify({ type: 'resize', cols: _cols, rows: _rows }),
      )
    }
  }, [])

  // ---- 重连 ----
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
