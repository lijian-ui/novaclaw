import { useState, useCallback, useRef, useEffect } from 'react'
import type { EditorTab, UseFileEditorReturn } from '@/types/fileEditor'
import { getFileWebSocket, onFileWsMessage, sendFileWs } from '@/hooks/useFileWs'

const SAVE_DEBOUNCE_MS = 1500

/** 检测 Tauri 环境 */
const isTauri = (): boolean =>
  typeof window !== 'undefined' && !!(window as any).__TAURI__?.invoke

/** Tauri invoke */
async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return (window as any).__TAURI__.invoke(cmd, args || {}) as Promise<T>
}

function getLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() || ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'tsx', js: 'javascript', jsx: 'jsx',
    html: 'html', htm: 'html', css: 'css', scss: 'scss',
    json: 'json', md: 'markdown', rs: 'rust', py: 'python',
    go: 'go', java: 'java', yaml: 'yaml', yml: 'yaml',
    toml: 'toml', xml: 'xml', svg: 'xml', sql: 'sql', sh: 'bash', bash: 'bash', zsh: 'bash',
  }
  return map[ext] || ext
}

export function useFileEditor(): UseFileEditorReturn {
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activePath, setActivePath] = useState<string | null>(null)
  const [connected, setConnected] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingSaveRef = useRef<{ path: string; content: string } | null>(null)
  const tabsRef = useRef<EditorTab[]>(tabs)
  tabsRef.current = tabs
  const wsReadyRef = useRef(false)

  // ---- 共享 WebSocket 消息监听（浏览器模式）----
  useEffect(() => {
    if (isTauri()) {
      setConnected(true)
      return
    }

    // 初始化共享 WS
    getFileWebSocket().then(() => {
      wsReadyRef.current = true
      setConnected(true)
    }).catch(() => {
      setError('WebSocket 连接失败')
    })

    // 监听 file_changed 事件（外部修改时自动更新）
    const unsub = onFileWsMessage((msg) => {
      if (msg.type === 'file_changed') {
        const path = msg.path as string
        const change = msg.change as string
        const content = msg.content as string
        if (change === 'changed' || change === 'deleted') {
          setTabs(prev => prev.map(tab => {
            if (tab.path === path && !tab.dirty) {
              return { ...tab, content: content || tab.content, initialContent: content || tab.content }
            }
            return tab
          }))
        }
      }
    })

    return unsub
  }, [])

  /** 通过共享 WS 读取文件（等待连接就绪） */
  const readFileContent = useCallback(async (path: string): Promise<string> => {
    if (isTauri()) {
      return tauriInvoke<string>('read_file', { path })
    }
    // 等待共享 WS 就绪
    await getFileWebSocket()

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('读取超时')), 10000)
      const handler = (event: MessageEvent) => {
        try {
          const msg = JSON.parse(event.data)
          if (msg.type === 'read_result' && msg.path === path) {
            clearTimeout(timeout)
            ;(event.target as EventTarget)?.removeEventListener?.('message', handler as EventListener)
            if (msg.success) resolve(msg.content)
            else reject(new Error(msg.message || '读取失败'))
          }
        } catch { /* ignore */ }
      }
      // 直接用 WebSocket 发 read 请求
      getFileWebSocket().then(ws => {
        ws.addEventListener('message', handler)
        ws.send(JSON.stringify({ type: 'read', path }))
      })
    })
  }, [])

  /** 通过共享 WS 保存文件 */
  const saveFileContent = useCallback(async (path: string, content: string): Promise<void> => {
    if (isTauri()) {
      await tauriInvoke('write_file', { path, content })
      return
    }
    await sendFileWs({ type: 'write', path, content })
  }, [])

  /** 打开文件 */
  const openFile = useCallback(async (path: string) => {
    const existing = tabsRef.current.find(t => t.path === path)
    if (existing) {
      setActivePath(path)
      return
    }

    try {
      const content = await readFileContent(path)
      const name = path.split(/[/\\]/).pop() || path
      const newTab: EditorTab = {
        path, name, content, initialContent: content,
        dirty: false, language: getLanguage(path),
      }
      setTabs(prev => [...prev, newTab])
      setActivePath(path)
    } catch (err) {
      console.error('打开文件失败:', err)
    }
  }, [readFileContent])

  const closeTab = useCallback((path: string) => {
    setTabs(prev => {
      const idx = prev.findIndex(t => t.path === path)
      const next = prev.filter(t => t.path !== path)
      if (path === activePath && next.length > 0) {
        setActivePath(next[Math.min(idx, next.length - 1)].path)
      } else if (next.length === 0) {
        setActivePath(null)
      }
      return next
    })
  }, [activePath])

  const updateContent = useCallback((content: string) => {
    if (!activePath) return
    setTabs(prev => prev.map(tab =>
      tab.path === activePath ? { ...tab, content, dirty: content !== tab.initialContent } : tab
    ))
  }, [activePath])

  const saveCurrent = useCallback(async () => {
    if (!activePath) return
    const tab = tabsRef.current.find(t => t.path === activePath)
    if (!tab?.dirty) return
    try {
      await saveFileContent(tab.path, tab.content)
      setTabs(prev => prev.map(t =>
        t.path === tab.path ? { ...t, dirty: false, initialContent: t.content } : t
      ))
    } catch (err) {
      console.error('保存文件失败:', err)
    }
  }, [activePath, saveFileContent])

  // 防抖自动保存
  useEffect(() => {
    if (!activePath) return
    const tab = tabsRef.current.find(t => t.path === activePath)
    if (!tab?.dirty) return
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    pendingSaveRef.current = { path: tab.path, content: tab.content }

    saveTimerRef.current = setTimeout(() => {
      const pending = pendingSaveRef.current
      if (pending) {
        saveFileContent(pending.path, pending.content)
          .then(() => {
            setTabs(prev => prev.map(t =>
              t.path === pending.path ? { ...t, dirty: false, initialContent: t.content } : t
            ))
          }).catch(() => {})
        pendingSaveRef.current = null
      }
    }, SAVE_DEBOUNCE_MS)

    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    }
  }, [tabs, activePath, saveFileContent])

  const switchTab = useCallback((path: string) => setActivePath(path), [])

  return {
    tabs,
    activePath,
    activeTab: activePath ? tabs.find(t => t.path === activePath) || null : null,
    openFile, closeTab, updateContent, saveCurrent, switchTab,
    connected,
    error,
  }
}
