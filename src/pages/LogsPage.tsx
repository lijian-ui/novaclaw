import { useState, useEffect, useRef, useCallback } from 'react'
import { ArrowLeft, Info, AlertTriangle, XCircle, Bug, Terminal, Loader2, BugOff } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { getApiBase } from '@/hooks/useApi'

type LogLevel = 'all' | 'info' | 'warn' | 'error' | 'debug'

interface LogEntry {
  id: number
  timestamp: string
  level: string
  message: string
  task_id?: string | null
}

const WS_URL = 'ws://127.0.0.1:5173/ws/logs'

const levelConfig: Record<string, { label: string; icon: React.ElementType; color: string }> = {
  all: { label: '全部', icon: Terminal, color: '' },
  info: { label: '信息', icon: Info, color: 'text-blue-400' },
  warn: { label: '警告', icon: AlertTriangle, color: 'text-amber-400' },
  error: { label: '错误', icon: XCircle, color: 'text-red-400' },
  debug: { label: '调试', icon: Bug, color: 'text-foreground/50' },
}

const levelColors: Record<string, string> = {
  info: 'text-blue-400 before:bg-blue-400',
  warn: 'text-amber-400 before:bg-amber-400',
  error: 'text-red-400 before:bg-red-400',
  debug: 'text-foreground/50 before:bg-foreground/30',
}

/// 将后端日志级别映射为前端过滤级别 (INFO -> info)
function normalizeLevel(level: string): string {
  const l = level.toLowerCase()
  if (l === 'trace') return 'debug'
  return l
}

interface LogsPageProps {
  onBack?: () => void
}

export function LogsPage({ onBack }: LogsPageProps) {
  const { t } = useTranslation()
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [filter, setFilter] = useState<LogLevel>('all')
  const [loading, setLoading] = useState(true)
  const [connected, setConnected] = useState(false)
  const [debugMode, setDebugMode] = useState(() => localStorage.getItem('jeeves_log_debug') === 'true')
  const scrollRef = useRef<HTMLDivElement>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const idCounterRef = useRef(0)

  // 只加载一次历史日志
  const loadHistory = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetch(`${getApiBase()}/logs`)
      const data = await res.json()
      if (data.success && Array.isArray(data.data)) {
        setLogs(data.data.map((entry: any) => {
          idCounterRef.current += 1
          return {
            id: idCounterRef.current,
            timestamp: entry.timestamp || '',
            level: normalizeLevel(entry.level),
            message: entry.message || '',
            task_id: entry.task_id || null,
          }
        }))
      }
    } catch { /* ignore */ }
    setLoading(false)
  }, [])

  useEffect(() => { loadHistory() }, [loadHistory])

  // 连接到 WebSocket（延迟 100ms 避免 React 严格模式下的重复连接报错）
  useEffect(() => {
    let ws: WebSocket | null = null
    let cancelled = false
    const timer = setTimeout(() => {
      if (cancelled) return
      ws = new WebSocket(WS_URL)
      wsRef.current = ws
      ws.onopen = () => setConnected(true)
      ws.onclose = () => setConnected(false)
      ws.onerror = () => { /* onclose 会处理 */ }
      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data)
          if (msg.type === 'heartbeat' || msg.type === 'connected') return
          if (msg.type === 'log' && msg.data) {
            const entry = msg.data as {
              timestamp?: string; level?: string; message?: string; task_id?: string | null
            }
            if (entry.level && entry.message) {
              idCounterRef.current += 1
              const id = idCounterRef.current
              setLogs(prev => {
                const newEntry: LogEntry = {
                  id,
                  timestamp: entry.timestamp || new Date().toISOString().replace('T', ' ').slice(0, 19),
                  level: normalizeLevel(entry.level || 'info'),
                  message: entry.message || '',
                  task_id: entry.task_id || null,
                }
                const updated = [...prev, newEntry]
                return updated.length > 1000 ? updated.slice(-1000) : updated
              })
            }
          }
        } catch { /* ignore */ }
      }
    }, 100)

    return () => {
      cancelled = true
      clearTimeout(timer)
      if (ws) { ws.close(); ws.onclose = null; wsRef.current = null }
    }
  }, [])

  // 挂载时同步后端日志级别（如果之前开启了 Debug）
  useEffect(() => {
    if (debugMode) {
      fetch(`${getApiBase()}/logs/level`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ level: 'debug' }),
      }).catch(() => {})
    }
  }, [])

  // Debug 模式开关：切换后端日志级别 info ↔ debug
  const toggleDebugMode = useCallback(async () => {
    const next = !debugMode
    setDebugMode(next)
    localStorage.setItem('jeeves_log_debug', String(next))
    try {
      await fetch(`${getApiBase()}/logs/level`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ level: next ? 'debug' : 'info' }),
      })
    } catch { /* ignore */ }
  }, [debugMode])

  // Auto scroll（filter 变化或新日志到达时始终滚到底部）
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [filter, logs.length])

  const levels: LogLevel[] = ['all', 'info', 'warn', 'error', 'debug']
  const filteredLogs = filter === 'all' ? logs : logs.filter(l => l.level === filter)

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3.5 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('logsPage.title')}</span>
          <span className="text-[10px] text-foreground/30">{t('logsPage.logsCount', { count: logs.length })}</span>
        </div>
        <div className="flex items-center gap-2">
          {/* 连接状态 */}
          <span className={`text-[10px] px-1.5 py-0.5 rounded flex items-center gap-1 ${
            connected ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400'
          }`}>
            <span className={`w-1.5 h-1.5 rounded-full ${connected ? 'bg-green-400' : 'bg-red-400'}`} />
            {connected ? '已连接' : '未连接'}
          </span>
        </div>
      </div>

      {/* 日志级别过滤按钮（仅前端过滤） + Debug 模式开关 */}
      <div className="flex items-center gap-2 px-4 py-1.5 border-b border-border shrink-0 overflow-x-auto">
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
              {t(`logsPage.${lv}` as any)}
            </button>
          )
        })}

        <div className="w-px h-5 bg-border mx-1" />

        {/* Debug 模式开关 */}
        <button onClick={toggleDebugMode}
          className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs whitespace-nowrap transition-colors ${
            debugMode ? 'bg-amber-500/20 text-amber-400' : 'bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
          }`}>
          {debugMode ? <Bug className="w-3.5 h-3.5" /> : <BugOff className="w-3.5 h-3.5" />}
          {debugMode ? 'Debug 已开启' : 'Debug 已关闭'}
        </button>
      </div>

      {/* 日志内容 */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <Loader2 className="w-5 h-5 animate-spin text-foreground/30" />
          </div>
        ) : filteredLogs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Terminal className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">{t('logsPage.noMatchingLogs')}</p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {filteredLogs.map(log => {
              const lvColor = levelColors[log.level] || levelColors.info
              return (
                <div key={log.id} className="flex items-start gap-3 px-4 py-2 hover:bg-foreground/[0.02] transition-colors">
                  <div className={`shrink-0 w-2 h-2 rounded-full mt-1.5 ${lvColor.split(' ')[1] || 'bg-foreground/20'}`} />
                  <span className="text-[11px] text-foreground/30 font-mono shrink-0 w-[130px]">{log.timestamp}</span>
                  <span className={`text-[11px] font-medium shrink-0 w-[40px] ${lvColor.split(' ')[0]}`}>
                    {t(`logsPage.${log.level}` as any)}
                  </span>
                  <span className="text-xs text-foreground/70 break-all">{log.message}</span>
                  {log.task_id && (
                    <span className="text-[10px] text-foreground/30 font-mono shrink-0 ml-auto" title={log.task_id}>
                      #{log.task_id.slice(0, 8)}
                    </span>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
