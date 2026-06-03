import { useEffect, useRef, useState, useCallback } from 'react'
import { ScrollText, Trash2, Info, AlertTriangle, XCircle, Bug } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { getApiBase } from '@/hooks/useApi'

function getLogsWsUrl(): string {
  const apiBase = getApiBase()
  if (apiBase.startsWith('http://') || apiBase.startsWith('https://')) {
    return apiBase.replace(/^http/, 'ws').replace(/\/api$/, '/ws/logs')
  }
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}/ws/logs`
}

type LogLevel = 'all' | 'info' | 'warn' | 'error' | 'debug'

interface LogEntry {
  timestamp: string
  level: string
  module: string
  message: string
  task_id?: string | null
}

const levelConfig: Record<string, { label: string; color: string; icon: React.ElementType }> = {
  all: { label: '全部', color: '', icon: ScrollText },
  info: { label: '信息', color: 'text-blue-500', icon: Info },
  warn: { label: '警告', color: 'text-yellow-500', icon: AlertTriangle },
  error: { label: '错误', color: 'text-red-500', icon: XCircle },
  debug: { label: '调试', color: 'text-gray-500', icon: Bug },
}

function normalizeLevel(level: string): string {
  const l = level.toLowerCase()
  if (l === 'trace') return 'debug'
  return l
}

export function LogPanel() {
  const [connected, setConnected] = useState(false)
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [filter, setFilter] = useState<LogLevel>('all')
  const scrollRef = useRef<HTMLDivElement>(null)
  const wsRef = useRef<WebSocket | null>(null)

  useEffect(() => {
    const ws = new WebSocket(getLogsWsUrl())
    wsRef.current = ws

    ws.onopen = () => setConnected(true)
    ws.onclose = () => setConnected(false)
    ws.onerror = () => setConnected(false)

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data)
        if (msg.type === 'heartbeat' || msg.type === 'connected' || msg.type === 'lagged') return

        if (msg.type === 'log' && msg.data) {
          const entry = msg.data as LogEntry
          if (entry.level && entry.message) {
            setLogs(prev => {
              const updated = [...prev, {
                timestamp: entry.timestamp || new Date().toISOString(),
                level: normalizeLevel(entry.level),
                module: entry.module || 'System',
                message: entry.message,
                task_id: entry.task_id || null,
              }]
              return updated.length > 500 ? updated.slice(-500) : updated
            })
          }
        }
      } catch { /* ignore */ }
    }

    return () => { ws.close(); wsRef.current = null }
  }, [])

  // Auto scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [logs])

  const clearLogs = useCallback(() => {
    setLogs([])
  }, [])

  const getLevelColor = (level: string) => {
    const cfg = levelConfig[normalizeLevel(level)]
    return cfg?.color || 'text-gray-500'
  }

  const formatTime = (timestamp: string) => {
    try {
      return new Date(timestamp).toLocaleTimeString('zh-CN', {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
      })
    } catch {
      return timestamp
    }
  }

  const filteredLogs = filter === 'all'
    ? logs
    : logs.filter(l => normalizeLevel(l.level) === filter)

  const levels: LogLevel[] = ['all', 'info', 'warn', 'error', 'debug']

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
          <Button variant="outline" size="sm" onClick={clearLogs}>
            <Trash2 className="w-3 h-3 mr-1" />
            清空
          </Button>
        </div>
      </CardHeader>
      <div className="flex items-center gap-1 px-3 pb-2 overflow-x-auto">
        {levels.map(lv => {
          const isActive = filter === lv
          const cfg = levelConfig[lv]
          const Icon = cfg.icon
          return (
            <button key={lv} onClick={() => setFilter(lv)}
              className={`flex items-center gap-1 px-2 py-1 rounded text-[10px] whitespace-nowrap transition-colors ${
                isActive ? 'bg-blue-500/20 text-blue-600 font-medium' : 'text-gray-500 hover:bg-gray-100'
              }`}>
              {lv !== 'all' && <Icon className="w-3 h-3" />}
              {cfg.label}
            </button>
          )
        })}
      </div>
      <CardContent className="flex-1 overflow-hidden p-2">
        <div ref={scrollRef} className="h-full overflow-auto space-y-1">
          {filteredLogs.length === 0 ? (
            <div className="text-center text-gray-500 py-4">等待日志...</div>
          ) : (
            filteredLogs.map((log, index) => (
              <div key={index} className="text-xs flex gap-2 py-1 border-b border-border last:border-0">
                <span className="text-gray-400 whitespace-nowrap">
                  {formatTime(log.timestamp)}
                </span>
                <span className={`font-medium ${getLevelColor(log.level)} min-w-[40px]`}>
                  [{log.level.toUpperCase()}]
                </span>
                <span className="text-gray-700 break-all flex-1">{log.message}</span>
                {log.task_id && (
                  <span className="text-[10px] text-gray-400 font-mono shrink-0">
                    #{log.task_id.slice(0, 8)}
                  </span>
                )}
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  )
}
