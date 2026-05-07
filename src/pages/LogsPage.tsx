import { useState, useEffect, useRef } from 'react'
import { ArrowLeft, Info, AlertTriangle, XCircle, Bug, Terminal, Loader2 } from 'lucide-react'

type LogLevel = 'all' | 'info' | 'warn' | 'error' | 'debug'

interface LogEntry {
  id: number
  timestamp: string
  level: string
  module: string
  message: string
}

const WS_LOGS = 'ws://127.0.0.1:3000/ws/logs'
const API_LOGS = 'http://127.0.0.1:3000/api/logs'

const levelConfig: Record<string, { label: string; icon: React.ElementType; color: string }> = {
  all: { label: '全部', icon: Terminal, color: '' },
  info: { label: '信息', icon: Info, color: 'text-blue-400' },
  warn: { label: '警告', icon: AlertTriangle, color: 'text-amber-400' },
  error: { label: '错误', icon: XCircle, color: 'text-red-400' },
  debug: { label: '调试', icon: Bug, color: 'text-foreground/50' },
}

const levelColors: Record<string, string> = {
  all: '',
  info: 'text-blue-400 before:bg-blue-400',
  warn: 'text-amber-400 before:bg-amber-400',
  error: 'text-red-400 before:bg-red-400',
  debug: 'text-foreground/50 before:bg-foreground/30',
}

interface LogsPageProps {
  onBack?: () => void
}

export function LogsPage({ onBack }: LogsPageProps) {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [filter, setFilter] = useState<LogLevel>('all')
  const [loading, setLoading] = useState(true)
  const scrollRef = useRef<HTMLDivElement>(null)
  const wsRef = useRef<WebSocket | null>(null)

  // Load initial logs from API
  useEffect(() => {
    fetch(API_LOGS)
      .then(r => r.json())
      .then(data => {
        if (data.entries) {
          setLogs(data.entries)
          setLoading(false)
        }
      })
      .catch(() => setLoading(false))

    // Connect to WebSocket for real-time logs
    const ws = new WebSocket(WS_LOGS)
    wsRef.current = ws
    ws.onmessage = (event) => {
      try {
        const entry = JSON.parse(event.data)
        if (entry.level && entry.message) {
          setLogs(prev => {
            const newEntry: LogEntry = {
              id: Date.now(),
              timestamp: entry.timestamp || new Date().toISOString().replace('T', ' ').slice(0, 19),
              level: (entry.level || 'INFO').toLowerCase(),
              module: entry.module || 'System',
              message: entry.message,
            }
            const updated = [...prev, newEntry]
            return updated.length > 500 ? updated.slice(-500) : updated
          })
        }
      } catch {}
    }

    return () => { ws.close() }
  }, [])

  // Auto scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [logs])

  const levels: LogLevel[] = ['all', 'info', 'warn', 'error', 'debug']
  const filteredLogs = filter === 'all' ? logs : logs.filter(l => l.level === filter)

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">系统日志</span>
          <span className="text-[10px] text-foreground/30">{logs.length} 条</span>
        </div>
      </div>

      <div className="flex items-center gap-2 px-4 py-3 border-b border-border shrink-0 overflow-x-auto">
        {levels.map(lv => {
          const isActive = filter === lv
          const cfg = levelConfig[lv]
          const Icon = cfg.icon
          return (
            <button key={lv} onClick={() => setFilter(lv)}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs whitespace-nowrap transition-colors ${
                isActive ? 'bg-blue-500/20 text-blue-400' : 'bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
              }`}>
              {lv !== 'all' && <Icon className={`w-3.5 h-3.5 ${cfg.color}`} />}
              {cfg.label}
            </button>
          )
        })}
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <Loader2 className="w-5 h-5 animate-spin text-foreground/30" />
          </div>
        ) : filteredLogs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Terminal className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">暂无匹配的日志</p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {filteredLogs.map(log => {
              const lvColor = levelColors[log.level] || levelColors.info
              return (
                <div key={log.id} className="flex items-start gap-3 px-4 py-2 hover:bg-foreground/[0.02] transition-colors">
                  <div className={`shrink-0 w-2 h-2 rounded-full mt-1.5 ${lvColor.split(' ')[1] || 'bg-foreground/20'}`} />
                  <span className="text-[11px] text-foreground/30 font-mono shrink-0 w-[130px]">{log.timestamp}</span>
                  <span className="text-[11px] text-foreground/40 font-mono shrink-0 w-[70px]">{log.module}</span>
                  <span className={`text-[11px] font-medium shrink-0 w-[40px] ${lvColor.split(' ')[0]}`}>
                    {levelConfig[log.level]?.label || log.level.toUpperCase()}
                  </span>
                  <span className="text-xs text-foreground/70 break-all">{log.message}</span>
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
