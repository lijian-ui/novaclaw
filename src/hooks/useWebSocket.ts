import { useState, useCallback, useEffect, useRef } from 'react'

export type WebSocketMessage = {
  type: string
  data: unknown
}

export function useWebSocket(url: string) {
  const [connected, setConnected] = useState(false)
  const [messages, setMessages] = useState<WebSocketMessage[]>([])
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectAttemptsRef = useRef(0)
  const maxReconnectAttempts = 5

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      return
    }

    const ws = new WebSocket(url)
    
    ws.onopen = () => {
      setConnected(true)
      reconnectAttemptsRef.current = 0
    }

    ws.onmessage = (event) => {
      try {
        const message: WebSocketMessage = JSON.parse(event.data)
        setMessages(prev => [...prev, message])
      } catch {
        setMessages(prev => [...prev, { type: 'raw', data: event.data }])
      }
    }

    ws.onerror = () => {
      setConnected(false)
    }

    ws.onclose = () => {
      setConnected(false)
      
      if (reconnectAttemptsRef.current < maxReconnectAttempts) {
        const delay = Math.pow(2, reconnectAttemptsRef.current) * 1000
        setTimeout(() => {
          reconnectAttemptsRef.current += 1
          connect()
        }, delay)
      }
    }

    wsRef.current = ws
  }, [url])

  const send = useCallback((message: WebSocketMessage) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message))
    }
  }, [])

  const disconnect = useCallback(() => {
    wsRef.current?.close()
    wsRef.current = null
    setConnected(false)
  }, [])

  useEffect(() => {
    connect()

    return () => {
      disconnect()
    }
  }, [connect, disconnect])

  const clearMessages = useCallback(() => {
    setMessages([])
  }, [])

  return {
    connected,
    messages,
    send,
    disconnect,
    clearMessages,
  }
}