import { useState, useEffect } from 'react'
import { Plus, Trash2, MessageSquare } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { useChat } from '@/contexts/ChatContext'
import { useApi } from '@/hooks/useApi'
import type { Session } from '@/types'

export function SessionList() {
  const [sessions, setSessions] = useState<Session[]>([])
  const [deleteTarget, setDeleteTarget] = useState<Session | null>(null)
  const { currentSession, setCurrentSession, setMessages } = useChat()
  const { listSessions, createSession, deleteSession, getMessages } = useApi()

  useEffect(() => {
    loadSessions()
  }, [])

  const loadSessions = async () => {
    try {
      const result = await listSessions()
      if (Array.isArray(result)) {
        setSessions(result)
      } else {
        setSessions([])
      }
    } catch (error) {
      console.error('Failed to load sessions:', error)
      setSessions([])
    }
  }

  const handleCreateSession = async () => {
    try {
      const session = await createSession('New Session')
      if (session && session.id) {
        setSessions(prev => [session, ...prev])
        setCurrentSession(session)
        setMessages([])
      }
    } catch (error) {
      console.error('Failed to create session:', error)
    }
  }

  const handleSelectSession = async (session: Session) => {
    setCurrentSession(session)
    try {
      const messages = await getMessages(session.id)
      if (Array.isArray(messages)) {
        setMessages(messages)
      } else {
        setMessages([])
      }
    } catch (error) {
      console.error('Failed to load messages:', error)
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
    } catch (error) {
      console.error('Failed to delete session:', error)
    }
    setDeleteTarget(null)
  }

  const formatDate = (dateString: string) => {
    const date = new Date(dateString)
    return date.toLocaleDateString('zh-CN', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  }

  return (
    <div className="h-full flex flex-col">
      <div className="p-4 border-b border-border">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">会话列表</h2>
          <Button size="sm" onClick={handleCreateSession}>
            <Plus className="w-4 h-4 mr-1" />
            新建
          </Button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        {sessions.length === 0 ? (
          <div className="p-4 text-center text-muted-foreground">
            <MessageSquare className="w-12 h-12 mx-auto mb-2 opacity-50" />
            <p className="text-sm">暂无会话</p>
            <p className="text-xs mt-1">点击上方按钮创建</p>
          </div>
        ) : (
          <div className="p-2 space-y-1">
            {sessions.map((session) => (
              <div
                key={session.id}
                className={`group flex items-center justify-between p-3 rounded-lg cursor-pointer transition-colors ${
                  currentSession?.id === session.id
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent text-muted-foreground hover:text-accent-foreground'
                }`}
                onClick={() => handleSelectSession(session)}
              >
                <div className="flex-1 min-w-0">
                  <p className="font-medium truncate">{session.name}</p>
                  <p className="text-xs opacity-70">{formatDate(session.updated_at)}</p>
                </div>
                <Button
                  size="icon"
                  variant="ghost"
                  className="ml-2 opacity-0 group-hover:opacity-100"
                  onClick={(e) => {
                    e.stopPropagation()
                    setDeleteTarget(session)
                  }}
                >
                  <Trash2 className="w-4 h-4" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Delete Confirmation Dialog */}
      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/50" onClick={() => setDeleteTarget(null)} />
          <div className="relative bg-card border border-border rounded-xl shadow-2xl w-[320px] p-6">
            <h3 className="text-sm font-semibold text-foreground/90 mb-2">确认删除</h3>
            <p className="text-xs text-foreground/60 mb-1">
              确定要删除会话 "<span className="text-foreground/80 font-medium">{deleteTarget.name}</span>" 吗？
            </p>
            <p className="text-xs text-red-400/80 mb-4">
              此操作将同时删除关联的永久记忆，且无法恢复。
            </p>
            <div className="flex items-center justify-end gap-2">
              <button
                onClick={() => setDeleteTarget(null)}
                className="px-3 py-1.5 text-xs text-foreground/60 hover:text-foreground/80 hover:bg-foreground/10 rounded-md transition-colors"
              >
                取消
              </button>
              <button
                onClick={handleConfirmDelete}
                className="px-3 py-1.5 text-xs text-white bg-red-500 hover:bg-red-600 rounded-md transition-colors"
              >
                确认删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
