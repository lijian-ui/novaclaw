import { useState, useCallback, useRef, useEffect } from 'react'
import type { EditorTab, UseFileEditorReturn } from '@/types/fileEditor'
import { getFileWebSocket, onFileWsMessage } from '@/hooks/useFileWs'
import { getApiBase } from '@/hooks/useApi'

const SAVE_DEBOUNCE_MS = 1500

/** REST API 文件操作（统一走 Axum 后端，三平台通用） */
async function apiReadFile(path: string): Promise<string> {
  const res = await fetch(`${getApiBase()}/files/read`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  })
  const json = await res.json()
  if (!json.success) throw new Error(json.message || '读取失败')
  return json.data
}

async function apiWriteFile(path: string, content: string): Promise<void> {
  const res = await fetch(`${getApiBase()}/files/write`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, content }),
  })
  const json = await res.json()
  if (!json.success) throw new Error(json.message || '写入失败')
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
  const [error] = useState<string | null>(null)
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingSaveRef = useRef<{ path: string; content: string } | null>(null)
  const tabsRef = useRef<EditorTab[]>(tabs)
  tabsRef.current = tabs
  const wsReadyRef = useRef(false)

  // ---- 共享 WebSocket 消息监听（浏览器模式）----
  useEffect(() => {
    // 初始化共享 WS（文件编辑均走 REST API）
    getFileWebSocket().then(() => {
      wsReadyRef.current = true
      setConnected(true)
    }).catch(() => {
      // WebSocket 仅用于文件变更通知，非必须；核心文件操作走 REST
      setConnected(true)
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

  /** 通过 REST API 读取文件（三平台通用） */
  const readFileContent = useCallback(async (path: string): Promise<string> => {
    return apiReadFile(path)
  }, [])

  /** 通过 REST API 保存文件（三平台通用） */
  const saveFileContent = useCallback(async (path: string, content: string): Promise<void> => {
    await apiWriteFile(path, content)
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
