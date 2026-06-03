import { useState, useCallback, useRef } from 'react'
import axios from 'axios'
import type { Session, Message, Model, Skill, CronJob, Layout, ChatRequest, ChatResponse, Config, ProviderConfig } from '@/types'

// 后端地址：Tauri 桌面端直连 5173 端口，浏览器开发环境用 Vite proxy
export const getApiBase = (): string => {
  // Vite 的 import.meta.env.DEV 在开发模式下为 true，生产构建为 false
  // Tauri 桌面端使用生产构建，所以 DEV=false，走直连后端
  // 浏览器开发模式 DEV=true，走 Vite proxy
  const isDevelopment = typeof import.meta !== 'undefined' && (import.meta as any).env?.DEV === true

  if (isDevelopment) {
    return '/api'
  }
  // Tauri 桌面端或生产环境：直连后端
  return 'http://127.0.0.1:5173/api'
}

const api = axios.create({
  timeout: 30000,
})

// 请求拦截器：动态设置 baseURL
api.interceptors.request.use(config => {
  // 每次请求时动态检测是否在 Tauri 环境中
  if (!config.baseURL || config.baseURL === '/api') {
    config.baseURL = getApiBase()
  }
  console.log(`[API Request] ${config.method?.toUpperCase()} ${config.baseURL}${config.url}`)
  return config
})
api.interceptors.response.use(
  response => {
    console.log(`[API Response] ${response.config.method?.toUpperCase()} ${response.config.url} → ${response.status}`)
    return response
  },
  error => {
    if (axios.isCancel(error) || error?.name === 'CanceledError' || error?.code === 'ERR_CANCELED') {
      return Promise.reject(error)
    }
    console.error(`[API Error] ${error.config?.method?.toUpperCase()} ${error.config?.url} → ${error.response?.status || error.message}`)
    return Promise.reject(error)
  },
)

/** SSE 事件回调类型 */
export type SseCallbacks = {
  onChunk: (text: string) => void
  onDone: (result: { content?: string; sessionId?: string; inputTokens?: number; outputTokens?: number; cachedTokens?: number; cumulativeInputTokens?: number; cumulativeOutputTokens?: number; lastInputTokens?: number; lastOutputTokens?: number; cache_hit_rate?: number; cache_hit_tokens?: number }) => void
  onError: (err: string) => void
  onAgentStep?: (step: {
    stepType: string
    content: string
    toolName?: string
    toolResult?: string
    turn: number
    maxTurns: number
    /** 确认请求相关字段（仅 stepType = "approval_required" 时有值） */
    approval?: { operation_type?: string; tool_name?: string; arguments?: string; message?: string }
    approval_id?: string
  }) => void
}

/** 发起 SSE 流式聊天请求，返回 AbortController 用于取消 */
export function startChatStream(
  params: { message: string; model?: string; session_id?: string; workspace?: string; images?: string[]; agent_id?: string },
  callbacks: SseCallbacks,
): AbortController {
  const abortController = new AbortController()
  const { onChunk, onDone, onError, onAgentStep } = callbacks

  void (async () => {
    try {
      const response = await fetch(`${getApiBase()}/chat/stream`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(params),
        signal: abortController.signal,
      })

      if (!response.ok) {
        const text = await response.text().catch(() => '')
        onError(`请求失败 (${response.status}): ${text}`)
        return
      }

      const reader = response.body?.getReader()
      if (!reader) {
        onError('响应体为空')
        return
      }

      const decoder = new TextDecoder()
      let buffer = ''

      while (true) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })

        // 按 SSE 的双换行分隔符解析完整事件
        const parts = buffer.split('\n\n')
        buffer = parts.pop() || ''

        for (const part of parts) {
          const trimmed = part.trim()
          if (!trimmed || trimmed.startsWith(':')) continue

          // 提取 data: 行
          const dataLine = trimmed
            .split('\n')
            .find((line) => line.startsWith('data:'))
            ?.replace(/^data:\s*/, '')
          if (!dataLine) continue

          try {
            const parsed = JSON.parse(dataLine)
            const payload = parsed.data

            if (parsed.type === 'chunk') {
              onChunk(payload || '')
            } else if (parsed.type === 'agent_step') {
              onAgentStep?.({
                stepType: payload?.step_type || '',
                content: payload?.content || '',
                toolName: payload?.tool_name,
                toolResult: payload?.tool_result,
                turn: payload?.turn || 0,
                maxTurns: payload?.max_turns || 20,
                approval: payload?.approval,
                approval_id: payload?.approval_id,
              })
            } else if (parsed.type === 'done') {
              onDone({
                content: payload?.content || '',
                sessionId: payload?.session_id || undefined,
                inputTokens: payload?.input_tokens,
                outputTokens: payload?.output_tokens,
                cachedTokens: payload?.cached_tokens,
                cumulativeInputTokens: payload?.cumulative_input_tokens,
                cumulativeOutputTokens: payload?.cumulative_output_tokens,
                lastInputTokens: payload?.last_input_tokens,
                lastOutputTokens: payload?.last_output_tokens,
                cache_hit_rate: payload?.cache_hit_rate,
                cache_hit_tokens: payload?.cache_hit_tokens,
              })
            } else if (parsed.type === 'stopped') {
              onDone({
                content: '',
                sessionId: payload?.session_id || undefined,
              })
            } else if (parsed.type === 'error') {
              onError(payload?.message || '未知错误')
            }
        } catch (parseErr) {
          console.warn('[SSE] 解析事件数据失败:', dataLine, parseErr)
        }
        }
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        // 用户取消，忽略
        return
      }
      onError(err instanceof Error ? err.message : '连接失败')
    }
  })()

  return abortController
}

/** @-mention 查询文件列表 */
export async function queryMentions(
  workspace: string | undefined,
  query: string
): Promise<{ name: string; path: string; is_dir: boolean }[]> {
  try {
    const res = await fetch(`${getApiBase()}/mentions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ workspace, query }),
    })
    const body = await res.json()
    return body.success ? (body.data || []) : []
  } catch {
    return []
  }
}

/** @-mention 展开引用为文件内容 */
export async function expandMentions(content: string, workspace?: string): Promise<string> {
  try {
    const res = await fetch(`${getApiBase()}/mentions/expand`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content, workspace }),
    })
    const body = await res.json()
    return body.success ? (body.data || content) : content
  } catch {
    return content
  }
}

/** 取消正在进行的 SSE 流式生成 */
export async function cancelChatStream(sessionId: string): Promise<boolean> {
  try {
    const response = await fetch(`${getApiBase()}/chat/cancel`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ session_id: sessionId }),
    })
    const data = await response.json()
    return data.success === true
  } catch {
    return false
  }
}



export function useApi() {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const abortRef = useRef<AbortController | null>(null)

  const handleError = useCallback((err: unknown) => {
    if (axios.isAxiosError(err)) {
      setError(err.response?.data?.message || err.message)
    } else {
      setError(String(err))
    }
  }, [])

  // ---- HTTP Chat (非流式) ----
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
      // limit=100：只取最新 100 条，后端会返回最近的消息
      const response = await api.get(`/session`, { params: { session_id: sessionId, limit: 100 } })
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
      const response = await fetch(`${getApiBase()}/skills/upload`, {
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

  const toggleSkill = useCallback(async (id: string): Promise<boolean> => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.put(`/skills/${id}/toggle`)
      return response.data?.data?.enabled ?? false
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

  const saveProvider = useCallback(async (providers: ProviderConfig[], defaultModel?: string): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      const body: Record<string, any> = { providers }
      if (defaultModel !== undefined) {
        body.default_model = defaultModel
      }
      await api.put('/models-config', body)
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
    abortRef,
    // HTTP Chat
    chat,
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
    toggleSkill,
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