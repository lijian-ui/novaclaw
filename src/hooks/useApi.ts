import { useState, useCallback, useRef } from 'react'
import axios from 'axios'
import type { Session, Message, Model, Skill, CronJob, Layout, ChatRequest, ChatResponse, Config, ProviderConfig } from '@/types'

// 后端地址：Tauri 桌面端直连 3000 端口，浏览器开发环境用 Vite proxy
const isTauri = (): boolean =>
  typeof window !== 'undefined' && !!(window as any).__TAURI__?.invoke

const API_HOST = isTauri() ? 'http://127.0.0.1:3000' : ''
const API_BASE = `${API_HOST}/api`
const WS_BASE = isTauri()
  ? 'ws://127.0.0.1:3000/ws'
  : `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/ws`

const api = axios.create({
  baseURL: API_BASE,
  timeout: 30000,
})

// 调试日志：打印所有 API 请求的完整 URL 及响应状态
api.interceptors.request.use(config => {
  console.debug(`[API] ${config.method?.toUpperCase()} ${config.baseURL}${config.url}`)
  return config
})
api.interceptors.response.use(
  response => {
    console.debug(`[API] ${response.config.method?.toUpperCase()} ${response.config.url} → ${response.status}`)
    return response
  },
  error => {
    console.error(`[API] ${error.config?.method?.toUpperCase()} ${error.config?.url} → ${error.response?.status || error.message}`)
    return Promise.reject(error)
  },
)

export function useApi() {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const wsRef = useRef<WebSocket | null>(null)

  const handleError = useCallback((err: unknown) => {
    if (axios.isAxiosError(err)) {
      setError(err.response?.data?.message || err.message)
    } else {
      setError(String(err))
    }
  }, [])

  // ---- HTTP Chat ----
  const chat = useCallback(async (request: ChatRequest): Promise<ChatResponse> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.post('/chat', request)
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  // ---- WebSocket Chat Streaming ----
  // 后端协议：
  // 发送: {"type":"send","data":{"message":"...","model":"...","session_id":"..."}}
  // 接收: {"type":"chunk","data":"文本"}
  // 接收: {"type":"agent_step","data":{"step_type":"...","content":"...","tool_name":"...","tool_result":"...","turn":...,"max_turns":...}}
  // 接收: {"type":"done","data":{"session_id":"...","content":"...","iterations":...}}
  // 接收: {"type":"error","data":{"message":"..."}}
  const connectChatStream = useCallback((
    _sessionId: string | null,
    onChunk: (text: string) => void,
    /**
     * @param result - 包含 content 和可选的 sessionId
     * content: 后端的最终响应文本（仅最后一轮迭代），
     *         应优先使用 streamingContentRef.current（累积了所有轮次）
     * sessionId: 后端创建的会话 ID（首次对话时自动创建）
     */
    onDone: (result: { content?: string; sessionId?: string }) => void,
    onError: (err: string) => void,
    onAgentStep?: (step: { stepType: string; content: string; toolName?: string; toolResult?: string; turn: number; maxTurns: number }) => void,
  ) => {
    if (wsRef.current) {
      wsRef.current.close()
    }
    const ws = new WebSocket(`${WS_BASE}/chat`)
    wsRef.current = ws

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        const payload = data.data
        if (data.type === 'chunk') {
          onChunk(payload || data.data || '')
        } else if (data.type === 'agent_step') {
          onAgentStep?.({
            stepType: payload?.step_type || '',
            content: payload?.content || '',
            toolName: payload?.tool_name,
            toolResult: payload?.tool_result,
            turn: payload?.turn || 0,
            maxTurns: payload?.max_turns || 20,
          })
        } else if (data.type === 'done') {
          onDone({
            content: payload?.content || '',
            sessionId: payload?.session_id || undefined,
          })
        } else if (data.type === 'stopped') {
          // 用户打断停止，保留已输出的部分内容
          onDone({
            content: '',
            sessionId: payload?.session_id || undefined,
          })
        } else if (data.type === 'error') {
          onError(payload?.message || data.data?.message || '未知错误')
        }
      } catch {
        onChunk(event.data)
      }
    }

    ws.onerror = () => {
      onError('WebSocket 连接失败')
    }

    ws.onclose = () => {
      // 仅当 wsRef.current 仍然指向本 WebSocket 时才清空，
      // 防止异步触发时覆盖已创建的新连接
      if (wsRef.current === ws) {
        wsRef.current = null
      }
    }

    return ws
  }, [])

  // 后端协议：{"type":"send","data":{"message":"...","model":"...","session_id":"..."}}
  const sendChatMessage = useCallback((message: string, model?: string, sessionId?: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      const data: Record<string, string> = { message }
      if (model) data.model = model
      if (sessionId) data.session_id = sessionId
      wsRef.current.send(JSON.stringify({
        type: 'send',
        data,
      }))
    }
  }, [])

  // 发送停止指令：中断当前正在生成的流式响应
  // 后端收到后会取消 LLM 请求、保存已输出部分、发送 "stopped" 响应
  const stopChatStream = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'stop' }))
    }
  }, [])

  const disconnectChat = useCallback(() => {
    if (wsRef.current) {
      wsRef.current.close()
      wsRef.current = null
    }
  }, [])

  // ---- Models ----
  const listModels = useCallback(async (): Promise<Model[]> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/models')
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getModel = useCallback(async (id: string): Promise<Model> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get(`/models/${id}`)
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const listSessions = useCallback(async (): Promise<Session[]> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/sessions')
      // 后端返回 { success: true, data: [...] }
      return response.data?.data || []
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const createSession = useCallback(async (name: string, model?: string): Promise<Session> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.post('/sessions', { name, model })
      // 后端返回 { success: true, data: { id: ..., name: ... } }
      return response.data?.data || response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getSession = useCallback(async (id: string): Promise<Session> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get(`/session`, { params: { session_id: id } })
      return response.data?.data || response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const deleteSession = useCallback(async (id: string): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      await api.delete(`/session`, { params: { session_id: id } })
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getMessages = useCallback(async (sessionId: string): Promise<Message[]> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get(`/session`, { params: { session_id: sessionId } })
      // 后端返回 { success: true, data: [...] }
      return response.data?.data || []
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  // 消息持久化由后端 Agent 自动完成，前端仅用于查看历史消息
  const addMessage = useCallback(async (sessionId: string, role: string, content: string): Promise<Message> => {
    setLoading(true)
    setError(null)
    try {
      await api.get(`/sessions/${sessionId}/messages`)
      return { id: '', session_id: sessionId, role, content, created_at: new Date().toISOString() }
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const listSkills = useCallback(async (): Promise<Skill[]> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/skills')
      // 后端返回 { success: true, data: [...] }
      return response.data?.data || []
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const uploadSkill = useCallback(async (file: File): Promise<{ installed: number; errors: string[] }> => {
    setLoading(true)
    setError(null)
    try {
      // 读取文件内容为二进制，直接 POST 发送
      const arrayBuffer = await file.arrayBuffer()
      const response = await fetch(`${API_BASE}/skills/upload`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/zip' },
        body: arrayBuffer,
      })
      const result = await response.json()
      if (!result.success) {
        throw new Error(result.message || '上传失败')
      }
      return result.data || { installed: 0, errors: [] }
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getSkill = useCallback(async (id: string): Promise<Skill> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get(`/skills/${id}`)
      // 后端返回 { success: true, data: {...} }
      return response.data?.data || response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const deleteSkill = useCallback(async (id: string): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      await api.delete(`/skills/${id}`)
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const listCronJobs = useCallback(async (): Promise<CronJob[]> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/cron-jobs')
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const createCronJob = useCallback(async (name: string, schedule: string, payload: string): Promise<CronJob> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.post('/cron-jobs', { name, schedule, payload })
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getCronJob = useCallback(async (id: string): Promise<CronJob> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get(`/cron-jobs/${id}`)
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const deleteCronJob = useCallback(async (id: string): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      await api.delete(`/cron-jobs/${id}`)
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const updateCronJob = useCallback(async (id: string, data: Partial<CronJob>): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      await api.put(`/cron-jobs/${id}`, data)
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getLayout = useCallback(async (): Promise<Layout> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/layout')
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const saveLayout = useCallback(async (name: string, content: string): Promise<Layout> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.post('/layout', { name, content })
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const getConfig = useCallback(async (): Promise<Config> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/config')
      // 解包 { success, data } 中的 data 字段
      return response.data.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const updateConfig = useCallback(async (config: Partial<Config>): Promise<Config> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.put('/config', config)
      return response.data
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  // ---- Provider / Model Config ----
  const listProviders = useCallback(async (): Promise<ProviderConfig[]> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/models-config')
      // 解包 { success, data } 中的 data 字段
      return response.data.data?.providers || []
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  const saveProvider = useCallback(async (providers: ProviderConfig[]): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.get('/models-config')
      const currentConfig = response.data.data || {}
      await api.put('/models-config', { ...currentConfig, providers })
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  // ---- Default Model ----
  const getDefaultModel = useCallback(async (): Promise<string> => {
    try {
      const response = await api.get('/models-config')
      // 解包 { success, data } 中的 data 字段
      return response.data.data?.default_model || ''
    } catch {
      return ''
    }
  }, [])

  const setDefaultModel = useCallback(async (modelName: string): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      await api.put('/default-model', { model: modelName })
    } catch (err) {
      handleError(err)
      throw err
    } finally {
      setLoading(false)
    }
  }, [handleError])

  // ---- Test Provider Connection ----
  const testConnection = useCallback(async (params: {
    api_key: string
    base_url: string
    model: string
  }): Promise<{ success: boolean; message?: string }> => {
    try {
      const response = await api.post('/chat/test', params)
      return response.data
    } catch (err) {
      if (axios.isAxiosError(err) && err.response?.data) {
        return { success: false, message: err.response.data.message || '连接失败' }
      }
      return { success: false, message: '连接失败，请检查网络或配置' }
    }
  }, [])

  return {
    loading,
    error,
    // HTTP Chat
    chat,
    // WebSocket Chat
    connectChatStream,
    sendChatMessage,
    stopChatStream,
    disconnectChat,
    // Models
    listModels,
    getModel,
    // Sessions
    listSessions,
    createSession,
    getSession,
    deleteSession,
    getMessages,
    addMessage,
    // Skills
    listSkills,
    getSkill,
    deleteSkill,
    uploadSkill,
    // Cron
    listCronJobs,
    createCronJob,
    getCronJob,
    deleteCronJob,
    updateCronJob,
    // Layout
    getLayout,
    saveLayout,
    // Config
    getConfig,
    updateConfig,
    // Provider config
    listProviders,
    saveProvider,
    // Default model
    getDefaultModel,
    setDefaultModel,
    // Test connection
    testConnection,
  }
}