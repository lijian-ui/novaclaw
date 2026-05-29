import { createContext, useContext, useState, useCallback, ReactNode } from 'react'
import type { Session, Message, Model } from '@/types'

interface ChatContextType {
  currentSession: Session | null
  setCurrentSession: (session: Session | null) => void
  messages: Message[]
  setMessages: (messages: Message[]) => void
  addMessage: (message: Message) => void
  currentModel: Model | null
  setCurrentModel: (model: Model | null) => void
  isTyping: boolean
  setIsTyping: (typing: boolean) => void
  /** 会话列表版本号，每次创建或删除会话时递增，供侧边栏刷新列表 */
  sessionListVersion: number
  refreshSessionList: () => void
  /** 默认模型名称（双向同步用） */
  defaultModelName: string
  refreshModelKey: number
  setDefaultModelName: (name: string) => void
}

const ChatContext = createContext<ChatContextType | null>(null)

export function ChatProvider({ children }: { children: ReactNode }) {
  const [currentSession, setCurrentSession] = useState<Session | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [currentModel, setCurrentModel] = useState<Model | null>(null)
  const [isTyping, setIsTyping] = useState(false)
  const [sessionListVersion, setSessionListVersion] = useState(0)
  // 从 localStorage 同步初始化默认模型，避免异步加载前显示空值
  const [defaultModelName, setDefaultModelNameState] = useState(() => localStorage.getItem('jeeves-default-model') || '')
  const [refreshModelKey, setRefreshModelKey] = useState(0)

  const addMessage = useCallback((message: Message) => {
    setMessages(prev => [...prev, message])
  }, [])

  const refreshSessionList = useCallback(() => {
    setSessionListVersion(v => v + 1)
  }, [])

  const setDefaultModelName = useCallback((name: string) => {
    setDefaultModelNameState(name)
    if (name) localStorage.setItem('jeeves-default-model', name)
    setRefreshModelKey(k => k + 1)
  }, [])

  return (
    <ChatContext.Provider
      value={{
        currentSession,
        setCurrentSession,
        messages,
        setMessages,
        addMessage,
        currentModel,
        setCurrentModel,
        isTyping,
        setIsTyping,
        sessionListVersion,
        refreshSessionList,
        defaultModelName,
        refreshModelKey,
        setDefaultModelName,
      }}
    >
      {children}
    </ChatContext.Provider>
  )
}

export function useChat() {
  const context = useContext(ChatContext)
  if (!context) {
    throw new Error('useChat must be used within a ChatProvider')
  }
  return context
}