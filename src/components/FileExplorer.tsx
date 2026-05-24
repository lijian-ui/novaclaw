import { useState, useEffect, useCallback, useRef } from 'react'
import {
  ChevronRight, ChevronDown, Folder, FileCode, FileJson, FileType, FileImage,
  Loader2, FilePlus, FolderPlus, RefreshCw, FoldVertical,
  Trash2, Copy, Scissors, Clipboard, Pencil,
} from 'lucide-react'
import type { FileEntry } from '@/types/fileEditor'
import { getFileWebSocket, onFileWsMessage, sendFileWs } from '@/hooks/useFileWs'
import { getApiBase } from '@/hooks/useApi'
import { ConfirmDialog } from './ui/ConfirmDialog'

interface FileExplorerProps {
  onFileOpen?: (path: string) => void
  /** 自定义工作目录路径（来自于 ChatPanel 的文件夹选择） */
  customPath?: string
}

interface ContextMenuState {
  x: number
  y: number
  entry: FileEntry | null // null = on empty space
}

/** REST API 文件操作（统一走 Axum 后端，三平台通用） */
async function apiPost<T>(endpoint: string, body: unknown): Promise<T> {
  const res = await fetch(`${getApiBase()}${endpoint}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const json = await res.json()
  if (!json.success) throw new Error(json.message || '操作失败')
  return json.data as T
}

async function apiListDir(path: string): Promise<FileEntry[]> {
  return apiPost<FileEntry[]>('/files/list', { path })
}

async function apiWriteFile(path: string, content: string): Promise<void> {
  return apiPost('/files/write', { path, content })
}

async function apiMkdir(path: string): Promise<void> {
  return apiPost('/files/mkdir', { path })
}

async function apiRename(oldPath: string, newPath: string): Promise<void> {
  return apiPost('/files/rename', { old_path: oldPath, new_path: newPath })
}

async function apiDelete(path: string): Promise<void> {
  return apiPost('/files/delete', { path })
}

async function apiCopy(source: string, dest: string): Promise<void> {
  return apiPost('/files/copy', { source, dest })
}

async function apiGetPaths(): Promise<Record<string, string>> {
  const res = await fetch(`${getApiBase()}/paths`)
  const json = await res.json()
  if (!json.success) throw new Error(json.message || '获取路径失败')
  return json.data
}

function getFileExtColor(entry: FileEntry): string {
  if (entry.is_dir) return 'text-amber-400'
  const ext = (entry.extension || entry.name.split('.').pop() || '').toLowerCase()
  if (['ts', 'tsx', 'js', 'jsx'].includes(ext)) return 'text-blue-400'
  if (['json'].includes(ext)) return 'text-yellow-400'
  if (['css', 'scss', 'less'].includes(ext)) return 'text-purple-400'
  if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'ico'].includes(ext)) return 'text-green-400'
  return 'text-foreground/40'
}

function getFileIcon(entry: FileEntry) {
  if (entry.is_dir) return <Folder className="w-3.5 h-3.5 text-amber-400 shrink-0" />
  const ext = (entry.extension || entry.name.split('.').pop() || '').toLowerCase()
  if (['ts', 'tsx', 'js', 'jsx'].includes(ext)) return <FileCode className="w-3.5 h-3.5 text-blue-400 shrink-0" />
  if (['json'].includes(ext)) return <FileJson className="w-3.5 h-3.5 text-yellow-400 shrink-0" />
  if (['css', 'scss', 'less'].includes(ext)) return <FileType className="w-3.5 h-3.5 text-purple-400 shrink-0" />
  if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'ico'].includes(ext)) return <FileImage className="w-3.5 h-3.5 text-green-400 shrink-0" />
  return <FileCode className="w-3.5 h-3.5 text-foreground/40 shrink-0" />
}

function formatSize(size: number): string {
  if (size > 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`
  if (size > 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${size} B`
}

/**=============================================================
 * 右键菜单组件
 *=============================================================*/
function ContextMenu({
  x, y, entry, onClose, hasClipboard,
  onNewFile, onNewFolder, onRename, onDelete, onCopyPath,
  onCopy, onCut, onPaste,
}: {
  x: number; y: number; entry: FileEntry | null; onClose: () => void; hasClipboard: boolean
  onNewFile?: () => void; onNewFolder?: () => void
  onRename?: () => void; onDelete?: () => void; onCopyPath?: () => void
  onCopy?: () => void; onCut?: () => void; onPaste?: () => void
}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handle = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose()
    }
    // Use mousedown instead of click for immediate close
    document.addEventListener('mousedown', handle)
    return () => document.removeEventListener('mousedown', handle)
  }, [onClose])

  // Adjust position to avoid going off-screen
  const menuRef = useRef<HTMLDivElement>(null)
  const [adjustedX, setAdjustedX] = useState(x)
  const [adjustedY, setAdjustedY] = useState(y)

  useEffect(() => {
    if (menuRef.current) {
      const rect = menuRef.current.getBoundingClientRect()
      const w = window.innerWidth
      const h = window.innerHeight
      setAdjustedX(Math.min(x, w - rect.width - 8))
      setAdjustedY(Math.min(y, h - rect.height - 8))
    }
  }, [x, y])

  const menuStyle: React.CSSProperties = {
    position: 'fixed',
    left: adjustedX,
    top: adjustedY,
    zIndex: 9999,
    minWidth: 180,
    borderRadius: 8,
    padding: '4px 0',
    fontSize: 13,
    lineHeight: '20px',
    boxShadow: '0 4px 16px rgba(0,0,0,.18), 0 1px 3px rgba(0,0,0,.08)',
  }

  const divider = <div className="mx-2 my-1 h-px bg-foreground/10" />

  const menuItem = (label: string, icon: React.ReactNode, onClick?: () => void, disabled = false) => (
    <button
      disabled={disabled}
      className="w-full flex items-center gap-2.5 px-3 py-1.5 text-left transition-colors disabled:opacity-30 disabled:cursor-not-allowed hover:bg-blue-500/15 hover:text-blue-500"
      onClick={() => { onClick?.(); onClose() }}
    >
      <span className="w-4 h-4 shrink-0 flex items-center justify-center">{icon}</span>
      <span>{label}</span>
    </button>
  )

  const isDir = entry?.is_dir === true
  const isEmpty = !entry

  return (
    <div ref={menuRef} style={menuStyle} className="bg-mainbg border border-border">
      {/* 新建：目录 / 空区域 / 文件上（在父目录创建） */}
      {(isDir || isEmpty) && (
        <>
          {menuItem('新建文件', <FilePlus className="w-3.5 h-3.5" />, onNewFile)}
          {menuItem('新建文件夹', <FolderPlus className="w-3.5 h-3.5" />, onNewFolder)}
          {divider}
        </>
      )}

      {/* On any entry */}
      {entry && (
        <>
          {menuItem('复制', <Copy className="w-3.5 h-3.5" />, onCopy)}
          {menuItem('剪切', <Scissors className="w-3.5 h-3.5" />, onCut)}
          {divider}
          {menuItem('重命名', <Pencil className="w-3.5 h-3.5" />, onRename)}
          {menuItem('删除', <Trash2 className="w-3.5 h-3.5" />, onDelete)}
        </>
      )}
      {divider}
      {menuItem('粘贴', <Clipboard className="w-3.5 h-3.5" />, onPaste, !hasClipboard)}
      {divider}
      {entry && (
        <>
          {menuItem('复制路径', <Copy className="w-3.5 h-3.5" />, onCopyPath)}
        </>
      )}
    </div>
  )
}

/**=============================================================
 * 主组件
 *=============================================================*/
export function FileExplorer({ onFileOpen, customPath }: FileExplorerProps) {
  const [workspacePath, setWorkspacePath] = useState('')
  const [dirContents, setDirContents] = useState<Record<string, FileEntry[]>>({})
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(false)
  const pendingRootRef = useRef(false)

  // ---- 选中状态 ----
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [selectedIsDir, setSelectedIsDir] = useState(false)

  // ---- 新建 ----
  const [creatingType, setCreatingType] = useState<'file' | 'folder' | null>(null)
  const [creatingName, setCreatingName] = useState('')
  const [creatingParentPath, setCreatingParentPath] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  // ---- 重命名 ----
  const [renamingPath, setRenamingPath] = useState<string | null>(null)
  const [renamingName, setRenamingName] = useState('')
  const renameInputRef = useRef<HTMLInputElement>(null)

  // ---- 右键菜单 ----
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)
  // 用 ref 同步记录右键点击的条目路径，避免异步 state 竞争
  const contextEntryRef = useRef<{ path: string; isDir: boolean } | null>(null)

  // ---- 剪贴板 ----
  const clipboardRef = useRef<{ path: string; isCut: boolean } | null>(null)

  // ---- 删除确认对话框 ----
  const [deleteConfirm, setDeleteConfirm] = useState<{ path: string; name: string; parentDir: string } | null>(null)

  // ---- 初始加载工作目录路径 & 加载文件列表 ----
  // customPath 来自父组件（ChatPanel 的文件夹选择/持久化路径），优先级最高
  useEffect(() => {
    if (customPath) {
      setWorkspacePath(customPath)
      loadDir(customPath)
      return
    }
    apiGetPaths().then(paths => {
      if (paths.workspace_dir) {
        setWorkspacePath(paths.workspace_dir)
        loadDir(paths.workspace_dir)
      }
    }).catch(() => {})
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // 响应父组件传递的 customPath 变化，同步刷新目录列表
  useEffect(() => {
    if (customPath && customPath !== workspacePath) {
      setWorkspacePath(customPath)
      loadDir(customPath)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [customPath])

  // ---- 通过 WebSocket 接收文件变更和自动刷新 ----
  useEffect(() => {
    getFileWebSocket().then(ws => {
      ws.send(JSON.stringify({ type: 'get_workspace', path: '' }))
    })

    const unsub = onFileWsMessage((msg) => {
      if (msg.type === 'workspace_info') {
        const ws = msg.workspace as string
        setWorkspacePath(ws)
        pendingRootRef.current = true
        getFileWebSocket().then(s => {
          s.send(JSON.stringify({ type: 'list', path: ws }))
        })
      } else if (msg.type === 'list_result' && msg.success) {
        const path = msg.path as string
        const entries = msg.entries as FileEntry[]
        setDirContents(prev => ({ ...prev, [path]: entries || [] }))
        setLoading(false)
        if (pendingRootRef.current) {
          pendingRootRef.current = false
        }
      } else if (msg.type === 'dir_changed') {
        const changedPath = msg.path as string
        setDirContents(prev => {
          if (changedPath in prev || changedPath === workspacePath) {
            setTimeout(() => sendFileWs({ type: 'list', path: changedPath }), 0)
          }
          return prev
        })
      }
    })

    // 定时轮询：每 15 秒刷新根目录（检测外部创建的文件）
    const pollTimer = setInterval(() => {
      getFileWebSocket().then(s => {
        s.send(JSON.stringify({ type: 'list', path: '' }))
      })
    }, 15000)

    return () => {
      unsub()
      clearInterval(pollTimer)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // ---- 获取选中目录的父路径（用于创建时的 parent）----
  const getSelectedParent = useCallback((): string => {
    return selectedPath && selectedIsDir ? selectedPath : workspacePath
  }, [selectedPath, selectedIsDir, workspacePath])

  // ---- 加载目录 ----
  const loadDir = useCallback((dirPath: string) => {
    setLoading(true)
    apiListDir(dirPath).then(entries => {
      setDirContents(prev => ({ ...prev, [dirPath]: entries || [] }))
    }).catch(e => {
      console.error('加载目录失败:', e)
    }).finally(() => {
      setLoading(false)
    })
  }, [])

  // ---- 初始加载工作目录（REST API 兜底，WS 连接后也会加载） ----
  useEffect(() => {
    if (!workspacePath) return
    loadDir(workspacePath)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspacePath])

  // ---- 自动聚焦创建/重命名输入 ----
  useEffect(() => {
    if (creatingType) inputRef.current?.focus()
  }, [creatingType])

  useEffect(() => {
    if (renamingPath) renameInputRef.current?.focus()
  }, [renamingPath])

  // ---- 展开/折叠文件夹 ----
  const toggleDir = useCallback((path: string) => {
    setExpandedDirs(prev => {
      const n = new Set(prev)
      if (n.has(path)) { n.delete(path); return n }
      n.add(path)
      if (!dirContents[path]) {
        // Schedule load after state update
        setTimeout(() => loadDir(path), 0)
      }
      return n
    })
  }, [dirContents, loadDir])

  // ---- 点击条目 ----
  const handleClick = useCallback((e: React.MouseEvent, entry: FileEntry) => {
    e.stopPropagation()
    setSelectedPath(entry.path)
    setSelectedIsDir(entry.is_dir)
    if (entry.is_dir) {
      toggleDir(entry.path)
    } else if (onFileOpen) {
      onFileOpen(entry.path)
    }
  }, [onFileOpen, toggleDir])

  // ---- 右键菜单 ----
  const handleContextMenu = useCallback((e: React.MouseEvent, entry: FileEntry | null) => {
    e.preventDefault()
    e.stopPropagation()
    if (entry) {
      setSelectedPath(entry.path)
      setSelectedIsDir(entry.is_dir)
      contextEntryRef.current = { path: entry.path, isDir: entry.is_dir }
    } else {
      contextEntryRef.current = null
    }
    setContextMenu({ x: e.clientX, y: e.clientY, entry })
  }, [])

  const closeContextMenu = useCallback(() => setContextMenu(null), [])

  // ---- 新建文件/文件夹 ----
  const startCreating = useCallback((type: 'file' | 'folder', parentOverride?: string) => {
    const targetPath = parentOverride || getSelectedParent()
    setCreatingType(type)
    setCreatingName('')
    setCreatingParentPath(targetPath)

    // 自动展开目标目录
    if (targetPath !== workspacePath && !expandedDirs.has(targetPath)) {
      setExpandedDirs(prev => { const n = new Set(prev); n.add(targetPath); return n })
      if (!dirContents[targetPath]) {
        loadDir(targetPath)
      }
    }

    closeContextMenu()
  }, [getSelectedParent, expandedDirs, dirContents, loadDir, workspacePath, closeContextMenu])

  const cancelCreating = useCallback(() => {
    setCreatingType(null)
    setCreatingName('')
    setCreatingParentPath('')
  }, [])

  const confirmCreating = useCallback(async () => {
    if (!creatingType || !creatingName.trim()) return
    const sep = creatingParentPath.includes('\\') ? '\\' : '/'
    const name = creatingName.trim()
    const fullPath = creatingParentPath ? `${creatingParentPath}${sep}${name}` : name
    try {
      if (creatingType === 'file') {
        await apiWriteFile(fullPath, '')
      } else {
        await apiMkdir(fullPath)
      }
      cancelCreating()
      loadDir(creatingParentPath)
    } catch (e) {
      console.error('新建失败:', e)
    }
  }, [creatingType, creatingName, creatingParentPath, loadDir])

  // ---- 重命名 ----
  const startRename = useCallback(() => {
    const entryPath = contextEntryRef.current?.path
    if (!entryPath) return
    const entry = findEntryByPath(entryPath, workspacePath, dirContents)
    if (!entry) return
    setRenamingPath(entryPath)
    setRenamingName(entry.name)
    closeContextMenu()
  }, [workspacePath, dirContents, closeContextMenu])

  const cancelRename = useCallback(() => {
    setRenamingPath(null)
    setRenamingName('')
  }, [])

  const confirmRename = useCallback(async () => {
    if (!renamingPath || !renamingName.trim()) return
    const parentDir = renamingPath.substring(0, renamingPath.lastIndexOf(
      renamingPath.includes('\\') ? '\\' : '/'
    ))
    const sep = parentDir.includes('\\') ? '\\' : '/'
    const newPath = parentDir ? `${parentDir}${sep}${renamingName.trim()}` : renamingName.trim()
    try {
      await apiRename(renamingPath, newPath)
      cancelRename()
      loadDir(parentDir)
    } catch (e) {
      console.error('重命名失败:', e)
    }
  }, [renamingPath, renamingName, loadDir])

  // ---- 删除 ----
  const handleDelete = useCallback(() => {
    const entryPath = contextEntryRef.current?.path
    if (!entryPath) return
    const name = entryPath.split(entryPath.includes('\\') ? '\\' : '/').pop() || ''
    const parentDir = entryPath.substring(0, entryPath.lastIndexOf(
      entryPath.includes('\\') ? '\\' : '/'
    ))
    setDeleteConfirm({ path: entryPath, name, parentDir })
  }, [])

  const confirmDeleteAction = useCallback(async () => {
    if (!deleteConfirm) return
    const { path: entryPath, parentDir } = deleteConfirm
    setDeleteConfirm(null)
    try {
      await apiDelete(entryPath)
      setSelectedPath(null)
      setSelectedIsDir(false)
      closeContextMenu()
      loadDir(parentDir || workspacePath)
    } catch (e) {
      console.error('删除失败:', e)
    }
  }, [deleteConfirm, workspacePath, closeContextMenu, loadDir])

  // ---- 复制路径 ----
  const handleCopyPath = useCallback(() => {
    const entryPath = contextEntryRef.current?.path
    if (!entryPath) return
    navigator.clipboard.writeText(entryPath).catch(() => {})
    closeContextMenu()
  }, [closeContextMenu])

  // ---- 复制到剪贴板 ----
  const handleCopy = useCallback(() => {
    const entryPath = contextEntryRef.current?.path
    if (!entryPath) return
    clipboardRef.current = { path: entryPath, isCut: false }
    closeContextMenu()
  }, [closeContextMenu])

  // ---- 剪切 ----
  const handleCut = useCallback(() => {
    const entryPath = contextEntryRef.current?.path
    if (!entryPath) return
    clipboardRef.current = { path: entryPath, isCut: true }
    closeContextMenu()
  }, [closeContextMenu])

  // ---- 粘贴 ----
  const handlePaste = useCallback(async () => {
    const clip = clipboardRef.current
    if (!clip) return
    // 目标目录：当前右键的目录 或 选中目录 或 工作区根
    const ce = contextEntryRef.current
    const targetDir = (ce?.isDir && ce.path) ? ce.path
      : (selectedPath && selectedIsDir) ? selectedPath
      : workspacePath
    if (!targetDir) return
    const sep = targetDir.includes('\\') ? '\\' : '/'
    const srcName = clip.path.split(clip.path.includes('\\') ? '\\' : '/').pop() || ''
    let destName = srcName
    let destPath = `${targetDir}${sep}${destName}`

    if (clip.isCut) {
      // 剪切：同位置不操作
      if (destPath === clip.path) return
    } else {
      // 复制：检查目标路径是否已存在，存在则加 _copy 后缀
      const existingNames = new Set(
        (dirContents[targetDir] || []).map(e => e.name)
      )
      if (existingNames.has(destName)) {
        const dotIdx = destName.lastIndexOf('.')
        let base: string, ext: string
        if (dotIdx > 0) {
          base = destName.substring(0, dotIdx)
          ext = destName.substring(dotIdx)
        } else {
          base = destName
          ext = ''
        }
        destName = `${base}_copy${ext}`
        destPath = `${targetDir}${sep}${destName}`
      }
    }

    try {
      if (clip.isCut) {
        await apiRename(clip.path, destPath)
      } else {
        await apiCopy(clip.path, destPath)
      }
      clipboardRef.current = null
      closeContextMenu()
      loadDir(targetDir)
    } catch (e) {
      console.error('粘贴失败:', e)
    }
  }, [selectedPath, selectedIsDir, workspacePath, closeContextMenu, loadDir, dirContents])

  // ---- 辅助：在 dirContents 中查找 entry ----
  function findEntryByPath(
    path: string, _ws: string, contents: Record<string, FileEntry[]>
  ): FileEntry | undefined {
    for (const [, entries] of Object.entries(contents)) {
      const found = entries.find(e => e.path === path)
      if (found) return found
    }
    return undefined
  }

  // ---- 树形递归渲染 ----
  // 注意：CreatingInput 的渲染分两部分：
  //   - 已展开目录下的创建：通过 showCreatingHere 在条目子节点中渲染
  //   - 根级别创建（creatingParentPath===workspacePath）：由 JSX 中额外注入
  // 不使用 ref 做去重标记，避免跨递归/跨 re-render 的问题
  const renderTree = (entries: FileEntry[], depth: number, _parentPath: string) => {
    const items: React.ReactNode[] = []

    for (const entry of entries) {
      const isExpanded = expandedDirs.has(entry.path)
      const isSelected = selectedPath === entry.path
      const hasChildren = entry.is_dir
      const children = hasChildren && isExpanded && dirContents[entry.path]
      const showCreatingHere = creatingType && creatingParentPath === entry.path && isExpanded

      items.push(
        <div key={entry.path}>
          {/* 条目行 */}
          <div
            className={`flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer transition-colors group ${
              isSelected
                ? 'bg-blue-500/15 text-blue-400'
                : 'hover:bg-foreground/5 text-foreground/70'
            }`}
            style={{ paddingLeft: `${12 + depth * 16}px` }}
            onClick={(e) => handleClick(e, entry)}
            onContextMenu={(e) => handleContextMenu(e, entry)}
          >
            {/* 展开箭头 / 占位 */}
            {hasChildren ? (
              isExpanded
                ? <ChevronDown className={`w-3 h-3 shrink-0 ${isSelected ? 'text-blue-400' : 'text-foreground/30'}`} />
                : <ChevronRight className={`w-3 h-3 shrink-0 ${isSelected ? 'text-blue-400' : 'text-foreground/30'}`} />
            ) : (
              <span className="w-3 shrink-0" />
            )}

            {/* 重命名输入 */}
            {renamingPath === entry.path ? (
              <>
                <span className={`w-3.5 h-3.5 shrink-0 ${getFileExtColor(entry)}`}>{getFileIcon(entry)}</span>
                <input
                  ref={renameInputRef as React.RefObject<HTMLInputElement>}
                  value={renamingName}
                  onChange={e => setRenamingName(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter') { e.preventDefault(); confirmRename() }
                    if (e.key === 'Escape') { e.preventDefault(); cancelRename() }
                  }}
                  onBlur={() => { if (renamingName.trim()) confirmRename(); else cancelRename() }}
                  className="flex-1 bg-transparent text-xs text-foreground/80 outline-none border-b border-blue-400"
                  spellCheck={false}
                  onClick={e => e.stopPropagation()}
                />
              </>
            ) : (
              <>
                <span className="shrink-0">{getFileIcon(entry)}</span>
                <span className={`text-xs truncate flex-1 ${isSelected ? 'text-blue-400' : 'text-foreground/70'}`}>
                  {entry.name}
                </span>
              </>
            )}

            {/* 文件大小 */}
            {!entry.is_dir && renamingPath !== entry.path && (
              <span className="text-[10px] text-foreground/30 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                {formatSize(entry.size)}
              </span>
            )}
          </div>

          {/* 子节点 */}
          {hasChildren && isExpanded && (
            children ? (
              renderTree(children, depth + 1, entry.path)
            ) : (
              <div
                className="flex items-center gap-2 px-2 py-1 text-xs text-foreground/20"
                style={{ paddingLeft: `${12 + (depth + 1) * 16}px` }}
              >
                <Loader2 className="w-3 h-3 animate-spin" />
                <span>加载中...</span>
              </div>
            )
          )}

          {/* 新建输入：在匹配的展开目录下 */}
          {showCreatingHere && (
            <CreatingInput
              type={creatingType!}
              value={creatingName}
              onChange={setCreatingName}
              onConfirm={confirmCreating}
              onCancel={cancelCreating}
              depth={depth + 1}
              inputRef={inputRef as React.RefObject<HTMLInputElement>}
            />
          )}
        </div>
      )
    }

    return items
  }

  // ---- 点击空白区域取消选中 ----
  const handleEmptyAreaClick = useCallback(() => {
    setSelectedPath(null)
    setSelectedIsDir(false)
    closeContextMenu()
  }, [closeContextMenu])

  const rootEntries = workspacePath ? dirContents[workspacePath] : undefined

  return (
    <div className="h-full flex flex-col bg-mainbg select-none" onContextMenu={(e) => handleContextMenu(e, null)}>
      {/* 工具栏 */}
      <div className="flex items-center gap-1 px-4 py-4 border-b border-border shrink-0">
        <button title="新建文件" onClick={() => startCreating('file')}
          className={`p-1.5 rounded-md transition-colors ${creatingType ? 'text-blue-400 bg-blue-500/10' : 'text-foreground/40 hover:text-foreground/70 hover:bg-foreground/10'}`}
        ><FilePlus className="w-4 h-4" /></button>
        <button title="新建文件夹" onClick={() => startCreating('folder')}
          className={`p-1.5 rounded-md transition-colors ${creatingType ? 'text-blue-400 bg-blue-500/10' : 'text-foreground/40 hover:text-foreground/70 hover:bg-foreground/10'}`}
        ><FolderPlus className="w-4 h-4" /></button>
        <button title="刷新资源管理器" onClick={() => { if (workspacePath) loadDir(workspacePath) }}
          className="p-1.5 rounded-md text-foreground/40 hover:text-foreground/70 hover:bg-foreground/10 transition-colors"
        ><RefreshCw className="w-4 h-4" /></button>
        <button title="全部折叠" onClick={() => setExpandedDirs(new Set())}
          className="p-1.5 rounded-md text-foreground/40 hover:text-foreground/70 hover:bg-foreground/10 transition-colors"
        ><FoldVertical className="w-4 h-4" /></button>
      </div>

      {/* 文件树 */}
      <div className="flex-1 overflow-y-auto px-2 py-2" onClick={handleEmptyAreaClick}>
        {loading && !rootEntries ? (
          <div className="flex items-center justify-center py-8"><Loader2 className="w-4 h-4 animate-spin text-foreground/30" /></div>
        ) : rootEntries && rootEntries.length > 0 ? (
          <>
            {renderTree(rootEntries, 0, workspacePath)}
            {/* 根级别创建（不在任何已展开目录下） */}
            {creatingType && creatingParentPath === workspacePath && (
              <CreatingInput
                type={creatingType}
                value={creatingName}
                onChange={setCreatingName}
                onConfirm={confirmCreating}
                onCancel={cancelCreating}
                depth={0}
                inputRef={inputRef as React.RefObject<HTMLInputElement>}
              />
            )}
          </>
        ) : rootEntries && rootEntries.length === 0 && !creatingType ? (
          <div className="text-center py-8"><p className="text-xs text-foreground/30">空目录</p></div>
        ) : !rootEntries && !loading ? (
          <div className="text-center py-8"><p className="text-xs text-foreground/30">请先配置工作目录</p></div>
        ) : null}
      </div>

      {/* 右键菜单 */}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          entry={contextMenu.entry}
          onClose={closeContextMenu}
          hasClipboard={!!clipboardRef.current}
          onNewFile={() => {
            const ce = contextMenu.entry
            if (ce?.is_dir) startCreating('file', ce.path)
            else if (!ce) startCreating('file', workspacePath)
            else {
              // 在文件上右键 → 在父目录创建
              const parent = ce.path.substring(0, ce.path.lastIndexOf(ce.path.includes('\\') ? '\\' : '/'))
              startCreating('file', parent)
            }
          }}
          onNewFolder={() => {
            const ce = contextMenu.entry
            if (ce?.is_dir) startCreating('folder', ce.path)
            else if (!ce) startCreating('folder', workspacePath)
            else {
              const parent = ce.path.substring(0, ce.path.lastIndexOf(ce.path.includes('\\') ? '\\' : '/'))
              startCreating('folder', parent)
            }
          }}
          onRename={startRename}
          onDelete={handleDelete}
          onCopyPath={handleCopyPath}
          onCopy={handleCopy}
          onCut={handleCut}
          onPaste={handlePaste}
        />
      )}

      {/* 删除确认对话框 */}
      <ConfirmDialog
        open={!!deleteConfirm}
        title="确认删除"
        message={deleteConfirm ? `确定要删除 "${deleteConfirm.name}" 吗？此操作不可撤销。` : ''}
        confirmLabel="删除"
        cancelLabel="取消"
        variant="danger"
        onConfirm={confirmDeleteAction}
        onCancel={() => setDeleteConfirm(null)}
      />
    </div>
  )
}

/**=============================================================
 * 创建输入行子组件
 *=============================================================*/
function CreatingInput({
  type, inputRef, value, onChange, onConfirm, onCancel, depth,
}: {
  type: 'file' | 'folder'
  inputRef: React.Ref<HTMLInputElement>
  value: string
  onChange: (v: string) => void
  onConfirm: () => void
  onCancel: () => void
  depth: number
}) {
  return (
    <div
      className="flex items-center gap-2 px-2 py-1.5 rounded bg-blue-500/5"
      style={{ paddingLeft: `${12 + depth * 16}px` }}
    >
      {type === 'folder'
        ? <Folder className="w-3.5 h-3.5 text-amber-400 shrink-0" />
        : <FileCode className="w-3.5 h-3.5 text-blue-400 shrink-0" />}
      <input
        ref={inputRef}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={e => {
          if (e.key === 'Enter') { e.preventDefault(); onConfirm() }
          if (e.key === 'Escape') { e.preventDefault(); onCancel() }
        }}
        onBlur={() => { if (!value.trim()) onCancel() }}
        placeholder={type === 'folder' ? '文件夹名称' : '文件名'}
        className="flex-1 bg-transparent text-xs text-foreground/80 outline-none placeholder:text-foreground/30"
        spellCheck={false}
      />
    </div>
  )
}
