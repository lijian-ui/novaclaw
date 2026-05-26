import { useState, useEffect, useCallback, useRef } from 'react'
import { Plus, ChevronLeft, ChevronRight, Trash2, MessageSquare, Loader2 } from 'lucide-react'
import { useChat } from '@/contexts/ChatContext'
import { useApi } from '@/hooks/useApi'
import { useTranslation } from 'react-i18next'
import i18n from '../i18n'
import type { Session } from '@/types'
import appIcon from '@/assets/app-icon.png'

interface SidebarProps {
  collapsed: boolean
  onToggle: () => void
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const { t } = useTranslation()
  const [sessions, setSessions] = useState<Session[]>([])
  const [loading, setLoading] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<Session | null>(null)
  const { currentSession, setCurrentSession, setMessages, sessionListVersion, refreshSessionList } = useChat()
  const { listSessions, createSession, deleteSession, getMessages } = useApi()
  const autoOpenedRef = useRef(false)
  const hasSessionRef = useRef(false)

  const loadSessions = useCallback(async () => {
    setLoading(true)
    try {
      const result = await listSessions()
      if (Array.isArray(result)) {
        const seen = new Set<string>()
        const uniqueSessions = result.filter(session => {
          if (seen.has(session.id)) return false
          seen.add(session.id)
          return true
        })
        // 按更新时间降序排列，最新的在最前面
        uniqueSessions.sort((a, b) => {
          const aTime = new Date(a.updated_at || a.created_at).getTime()
          const bTime = new Date(b.updated_at || b.created_at).getTime()
          return bTime - aTime
        })
        setSessions(uniqueSessions)

        // 首次加载且当前未选中任何会话，自动打开最近一次的会话
        if (!autoOpenedRef.current && !hasSessionRef.current && uniqueSessions.length > 0) {
          autoOpenedRef.current = true
          hasSessionRef.current = true
          const latest = uniqueSessions[0]
          setCurrentSession(latest)
          setMessages([])
          try {
            const rawMessages = await getMessages(latest.id)
            if (Array.isArray(rawMessages)) {
              setMessages(rawMessages.map(m => ({ ...m, inputTokens: (m as any).input_tokens, outputTokens: (m as any).output_tokens })))
            }
          } catch {
            // 忽略消息加载错误
          }
        }
        if (uniqueSessions.length > 0) {
          hasSessionRef.current = true
        }
      } else {
        setSessions([])
      }
    } catch {
      setSessions([])
    }
    setLoading(false)
  }, [listSessions, setCurrentSession, setMessages, getMessages])

  useEffect(() => {
    loadSessions()
  }, [loadSessions, sessionListVersion])

  const handleCreateSession = async () => {
    try {
      const session = await createSession('新任务')
      if (session && session.id) {
        setSessions(prev => [session, ...prev])
        setCurrentSession(session)
        setMessages([])
        refreshSessionList()
      }
    } catch (error) {
      console.error('Failed to create session:', error)
    }
  }

  const handleSelectSession = async (session: Session) => {
    setCurrentSession(session)
    setMessages([])
    try {
      const rawMessages = await getMessages(session.id)
      if (Array.isArray(rawMessages)) {
        setMessages(rawMessages.map(m => ({ ...m, inputTokens: (m as any).input_tokens, outputTokens: (m as any).output_tokens })))
      } else {
        setMessages([])
      }
    } catch {
      setMessages([])
    }
  }

  const handleConfirmDelete = async () => {
    if (!deleteTarget) return
    try {
      await deleteSession(deleteTarget.id)
      setSessions(prev => prev.filter(s => s.id !== deleteTarget.id))
      if (currentSession?.id === deleteTarget.id) {
        setCurrentSession(null)
        setMessages([])
      }
      refreshSessionList()
    } catch (error) {
      console.error('Failed to delete session:', error)
    }
    setDeleteTarget(null)
  }

  const formatTime = (dateString: string) => {
    const date = new Date(dateString)
    const now = new Date()
    const diffMs = now.getTime() - date.getTime()
    const diffMin = Math.floor(diffMs / 60000)
    if (diffMin < 1) return t('sidebar.justNow')
    if (diffMin < 60) return t('sidebar.minutesAgo', { count: diffMin })
    const diffHour = Math.floor(diffMin / 60)
    if (diffHour < 24) return t('sidebar.hoursAgo', { count: diffHour })
    return date.toLocaleDateString(i18n.language, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
  }

  if (collapsed) {
    return (
      <aside className="h-full bg-sidebar flex flex-col items-center py-3 gap-4">
        <button
          onClick={onToggle}
          className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
        >
          <ChevronRight className="w-4 h-4 text-foreground/60" />
        </button>
        <img src={appIcon} alt="Jeeves" className="w-6 h-6 rounded cursor-pointer" />
      </aside>
    )
  }

  return (
    <aside className="h-full bg-sidebar flex flex-col">
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 py-3">
        <div className="flex items-center gap-2">
          <img src={appIcon} alt="Jeeves" className="w-6 h-6 rounded cursor-pointer hover:opacity-80 transition-opacity" />
        </div>
        <button
          onClick={onToggle}
          className="p-1 rounded hover:bg-foreground/10 transition-colors"
        >
          <ChevronLeft className="w-4 h-4 text-foreground/60" />
        </button>
      </div>

      {/* New Task Button */}
      <div className="px-3 mt-2">
        <button
          onClick={handleCreateSession}
          className="w-full flex items-center gap-2 px-3 py-2 rounded-md bg-foreground/5 hover:bg-foreground/10 transition-colors text-sm"
        >
          <Plus className="w-4 h-4 text-foreground/70" />
          <span className="text-foreground/80 font-medium">{t('sidebar.newTask')}</span>
        </button>
      </div>

      {/* Sessions List */}
      <div className="mt-3 flex-1 overflow-y-auto px-3">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="w-4 h-4 animate-spin text-foreground/30" />
          </div>
        ) : sessions.length === 0 ? (
          <div className="text-center py-8">
            <MessageSquare className="w-8 h-8 mx-auto mb-2 text-foreground/30" />
            <p className="text-xs text-foreground/40">{t('sidebar.noSessions')}</p>
            <p className="text-[10px] text-foreground/30 mt-1">{t('sidebar.createHint')}</p>
          </div>
        ) : (
          <div className="space-y-1">
            {sessions.map((session) => (
              <div
                key={session.id}
                className={`group flex items-start gap-2.5 px-3 py-2.5 rounded-md transition-colors cursor-pointer ${
                  currentSession?.id === session.id
                    ? 'bg-foreground/[0.08]'
                    : 'hover:bg-foreground/[0.06]'
                }`}
                onClick={() => handleSelectSession(session)}
              >
                <MessageSquare className="w-4 h-4 mt-0.5 text-foreground/40 shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-foreground/85 truncate" title={session.name}>
                    {session.name}
                  </p>
                  <p className="text-[11px] text-foreground/40 mt-0.5">
                    {formatTime(session.updated_at)}
                  </p>
                </div>
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    setDeleteTarget(session)
                  }}
                  className="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-red-500/20 transition-all shrink-0 mt-0.5"
                  title="删除会话"
                >
                  <Trash2 className="w-3.5 h-3.5 text-red-400" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Bottom label */}
      <div className="px-4 py-3 border-t border-border">
        <p className="text-[11px] text-foreground/30">{t('sidebar.tasks')}</p>
      </div>

      {/* Delete Confirmation Dialog */}
      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/50" onClick={() => setDeleteTarget(null)} />
          <div className="relative bg-card border border-border rounded-xl shadow-2xl w-[320px] p-6">
            <h3 className="text-sm font-semibold text-foreground/90 mb-2">{t('sidebar.confirmDelete')}</h3>
            <p className="text-xs text-foreground/60 mb-1">
              {t('sidebar.deleteMessage', { name: deleteTarget.name })}
            </p>
            <p className="text-xs text-red-400/80 mb-4">
              {t('sidebar.deleteWarning')}
            </p>
            <div className="flex items-center justify-end gap-2">
              <button
                onClick={() => setDeleteTarget(null)}
                className="px-3 py-1.5 text-xs text-foreground/60 hover:text-foreground/80 hover:bg-foreground/10 rounded-md transition-colors"
              >
                {t('common.cancel')}
              </button>
              <button
                onClick={handleConfirmDelete}
                className="px-3 py-1.5 text-xs text-white bg-red-500 hover:bg-red-600 rounded-md transition-colors"
              >
                {t('common.confirm')}
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  )
}
