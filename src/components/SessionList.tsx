import { useState, useEffect, useRef } from 'react'
import { Plus, Trash2, MessageSquare } from 'lucide-react'
import { useChat } from '@/contexts/ChatContext'
import { useApi } from '@/hooks/useApi'
import type { Session } from '@/types'

export function SessionList() {
  const [sessions, setSessions] = useState<Session[]>([])
  const [deleteTarget, setDeleteTarget] = useState<Session | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const { currentSession, setCurrentSession, setMessages, defaultModelName } = useChat()
  const { listSessions, createSession, deleteSession, getMessages, loading } = useApi()
  const initialLoadDone = useRef(false)

  useEffect(() => {
    loadSessions()
  }, [])

  useEffect(() => {
    if (!initialLoadDone.current || !currentSession) return
    if (!sessions.some(s => s.id === currentSession.id)) {
      loadSessions()
    }
  }, [currentSession])

  // 当 currentSession 指向一个列表中不存在的会话时（如首次对话自动创建），刷新列表
  useEffect(() => {
    if (!initialLoadDone.current || !currentSession) return
    if (!sessions.some(s => s.id === currentSession.id)) {
      loadSessions()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentSession])

  const loadSessions = async () => {
    setErrorMsg(null)
    try {
      const result = await listSessions()
      if (Array.isArray(result)) {
        setSessions(result)
        initialLoadDone.current = true
      } else {
        setSessions([])
      }
    } catch (error) {
      console.error('加载会话失败:', error)
      setErrorMsg(String(error))
      setSessions([])
      initialLoadDone.current = true
    }
  }

  const handleCreateSession = async () => {
    try {
      // 创建新任务，标题会在首次对话时根据用户消息自动生成
      // 继承当前选中的默认模型，避免新任务被后端分配默认值（如 gpt4）
      const session = await createSession('新任务', defaultModelName || undefined)
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
    setMessages([]) // 先清除旧消息，避免 ChatPanel 同步到旧数据
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
          <h2 className="text-lg font-semibold">任务列表</h2>
          <button
            onClick={handleCreateSession}
            className="w-full flex items-center gap-2 px-3 py-2 rounded-md bg-foreground/5 hover:bg-foreground/10 transition-colors text-sm"
          >
            <Plus className="w-4 h-4 text-foreground/70" />
            <span className="text-foreground/80 font-medium">新任务</span>
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        {errorMsg ? (
          <div className="p-4 text-center">
            <p className="text-xs text-red-400 mb-1">加载失败</p>
            <p className="text-[10px] text-foreground/40 break-all">{errorMsg}</p>
            <button onClick={loadSessions} className="mt-2 text-xs text-blue-400 hover:underline">重试</button>
          </div>
        ) : loading && sessions.length === 0 ? (
          <div className="p-4 text-center text-muted-foreground">
            <p className="text-sm">加载中...</p>
          </div>
        ) : sessions.length === 0 ? (
          <div className="p-4 text-center text-muted-foreground">
            <MessageSquare className="w-12 h-12 mx-auto mb-2 opacity-50" />
            <p className="text-sm">暂无会话</p>
            <p className="text-xs mt-1">发送消息自动创建，或点击上方按钮新建</p>
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
                <button
                  className="ml-2 opacity-0 group-hover:opacity-100 p-1 rounded-md hover:bg-foreground/10 transition-colors"
                  onClick={(e) => {
                    e.stopPropagation()
                    setDeleteTarget(session)
                  }}
                >
                  <Trash2 className="w-4 h-4 text-foreground/50" />
                </button>
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
