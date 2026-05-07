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
}

const ChatContext = createContext<ChatContextType | null>(null)

export function ChatProvider({ children }: { children: ReactNode }) {
  const [currentSession, setCurrentSession] = useState<Session | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [currentModel, setCurrentModel] = useState<Model | null>(null)
  const [isTyping, setIsTyping] = useState(false)

  const addMessage = useCallback((message: Message) => {
    setMessages(prev => [...prev, message])
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