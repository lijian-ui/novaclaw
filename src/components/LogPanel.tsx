import { useEffect, useRef } from 'react'
import { ScrollText, Trash2 } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { useWebSocket } from '@/hooks/useWebSocket'

interface LogEntry {
  timestamp: string
  level: string
  message: string
}

export function LogPanel() {
  const { connected, messages, clearMessages } = useWebSocket('ws://localhost:8080/ws/logs')
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [messages])

  const parseLog = (data: unknown): LogEntry | null => {
    if (typeof data === 'string') {
      try {
        const parsed = JSON.parse(data)
        return {
          timestamp: parsed.timestamp || new Date().toISOString(),
          level: parsed.level || 'INFO',
          message: parsed.message || data,
        }
      } catch {
        return {
          timestamp: new Date().toISOString(),
          level: 'INFO',
          message: data,
        }
      }
    }
    return null
  }

  const getLevelColor = (level: string) => {
    switch (level.toUpperCase()) {
      case 'ERROR':
        return 'text-red-500'
      case 'WARN':
        return 'text-yellow-500'
      case 'DEBUG':
        return 'text-blue-500'
      default:
        return 'text-gray-500'
    }
  }

  const formatTime = (timestamp: string) => {
    return new Date(timestamp).toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
  }

  return (
    <Card className="h-full flex flex-col">
      <CardHeader className="flex items-center justify-between">
        <CardTitle className="text-base flex items-center gap-2">
          <ScrollText className="w-5 h-5" />
          实时日志
        </CardTitle>
        <div className="flex items-center gap-2">
          <span className={`text-xs px-2 py-1 rounded-full ${connected ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}`}>
            {connected ? '已连接' : '未连接'}
          </span>
          <Button variant="outline" size="sm" onClick={clearMessages}>
            <Trash2 className="w-3 h-3 mr-1" />
            清空
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex-1 overflow-hidden p-2">
        <div ref={scrollRef} className="h-full overflow-auto space-y-1">
          {messages.length === 0 ? (
            <div className="text-center text-gray-500 py-4">等待日志...</div>
          ) : (
            messages.map((msg, index) => {
              const log = parseLog(msg.data)
              if (!log) return null
              return (
                <div key={index} className="text-xs flex gap-2 py-1 border-b border-border last:border-0">
                  <span className="text-gray-400 whitespace-nowrap">
                    {formatTime(log.timestamp)}
                  </span>
                  <span className={`font-medium ${getLevelColor(log.level)} min-w-[50px]`}>
                    [{log.level}]
                  </span>
                  <span className="text-gray-700 break-all">{log.message}</span>
                </div>
              )
            })
          )}
        </div>
      </CardContent>
    </Card>
  )
}