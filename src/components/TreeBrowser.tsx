import { useState, useEffect, useCallback } from 'react'
import { Folder, FolderOpen, ChevronRight, ChevronDown, RefreshCw, HardDrive } from 'lucide-react'
import { getApiBase } from '@/hooks/useApi'

interface DirEntry {
  name: string
  path: string
}

interface TreeNode {
  expanded: boolean
  loaded: boolean
  subdirs: DirEntry[]
  loading: boolean
}

interface TreeBrowserProps {
  initialPath: string
  onSelect: (path: string) => void
  onCancel: () => void
}

export function TreeBrowser({ initialPath, onSelect, onCancel }: TreeBrowserProps) {
  const [currentPath, setCurrentPath] = useState(initialPath)
  const [nodes, setNodes] = useState<Record<string, TreeNode>>({})
  const [error, setError] = useState('')
  const [isRoot, setIsRoot] = useState(initialPath === '/' || initialPath === '')

  // 请求后端加载子目录
  const loadSubdirs = useCallback(async (path: string, force: boolean = false) => {
    const existing = nodes[path]
    if (existing?.loaded && !force) return

    setNodes(prev => ({
      ...prev,
      [path]: { ...(prev[path] || { expanded: false, loaded: false, subdirs: [], loading: false }), loading: true },
    }))

    try {
      const res = await fetch(`${getApiBase()}/files/list-tree`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      })
      const json = await res.json()
      if (!json.success) {
        setError(json.message || '加载失败')
        setNodes(prev => ({ ...prev, [path]: { ...prev[path], loading: false } }))
        return
      }
      const { subdirs } = json.data
      setNodes(prev => ({
        ...prev,
        [path]: {
          expanded: true,
          loaded: true,
          subdirs: subdirs || [],
          loading: false,
        },
      }))
      setError('')
    } catch {
      setError('网络请求失败')
      setNodes(prev => ({ ...prev, [path]: { ...prev[path], loading: false } }))
    }
  }, [nodes])

  // 展开/折叠节点
  const toggleNode = useCallback((path: string) => {
    const node = nodes[path]
    if (!node?.expanded) {
      loadSubdirs(path)
    } else {
      setNodes(prev => ({ ...prev, [path]: { ...prev[path], expanded: false } }))
    }
  }, [nodes, loadSubdirs])

  // 进入目录
  const enterDir = useCallback((path: string) => {
    setCurrentPath(path)
    setIsRoot(false)
    loadSubdirs(path)
  }, [loadSubdirs])

  // 返回上级
  const goParent = useCallback(() => {
    const normalized = currentPath.replace(/[\\/]$/, '')
    const sep = normalized.includes('\\') ? '\\' : '/'
    const parent = normalized.split(sep).slice(0, -1).join(sep) || '/'
    setCurrentPath(parent)
    setIsRoot(parent === '/')
    loadSubdirs(parent)
  }, [currentPath, loadSubdirs])

  // 初始加载
  useEffect(() => {
    if (initialPath) {
      loadSubdirs(initialPath)
      setIsRoot(initialPath === '/')
    }
  }, [initialPath, loadSubdirs])

  // 渲染目录面包屑
  const pathSegments = (() => {
    const p = currentPath.replace(/\\/g, '/').replace(/\/$/, '')
    if (p === '/') return [{ name: '/', path: '/' }]
    const parts = p.split('/').filter(Boolean)
    const result: { name: string; path: string }[] = [{ name: '/', path: '/' }]
    let cumulative = ''
    for (const part of parts) {
      cumulative += '/' + part
      result.push({ name: part, path: cumulative })
    }
    return result
  })()

  const node = nodes[currentPath]
  const isLoading = node?.loading ?? true
  const subdirs = node?.subdirs ?? []

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onCancel}>
      <div
        className="bg-card border border-border rounded-xl shadow-2xl w-[420px] max-h-[520px] flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 标题栏 */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
          <span className="text-sm font-medium text-foreground/90">选择工作目录</span>
          <button
            className="p-1 rounded hover:bg-foreground/10 transition-colors"
            onClick={(e) => { e.stopPropagation(); loadSubdirs(currentPath, true) }}
            title="刷新"
          >
            <RefreshCw className="w-3.5 h-3.5 text-foreground/50" />
          </button>
        </div>

        {/* 面包屑导航 */}
        <div className="flex items-center gap-0.5 px-3 py-2 border-b border-border/50 shrink-0 overflow-x-auto text-[11px]">
          {pathSegments.map((seg, i) => (
            <span key={seg.path} className="flex items-center gap-0.5 shrink-0">
              {i > 0 && <ChevronRight className="w-3 h-3 text-foreground/30" />}
              <button
                className="text-foreground/60 hover:text-foreground/90 hover:underline transition-colors whitespace-nowrap"
                onClick={() => enterDir(seg.path)}
              >
                {seg.name}
              </button>
            </span>
          ))}
        </div>

        {/* 目录列表 */}
        <div className="flex-1 overflow-y-auto px-2 py-1 min-h-[200px]">
          {/* 返回上级 */}
          {!isRoot && (
            <button
              className="w-full flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-foreground/5 transition-colors text-left"
              onClick={goParent}
            >
              <FolderOpen className="w-4 h-4 text-foreground/40 shrink-0" />
              <span className="text-xs text-foreground/50">..</span>
            </button>
          )}

          {/* 加载中 */}
          {isLoading && (
            <div className="flex items-center gap-2 px-2 py-3 text-foreground/40">
              <div className="w-4 h-4 border-2 border-foreground/20 border-t-foreground/50 rounded-full animate-spin" />
              <span className="text-xs">加载中...</span>
            </div>
          )}

          {/* 空目录 */}
          {!isLoading && subdirs.length === 0 && (
            <div className="px-2 py-4 text-xs text-foreground/40 text-center">此目录下没有子目录</div>
          )}

          {/* 子目录列表 */}
          {!isLoading && subdirs.map((d) => {
            const childNode = nodes[d.path]
            const isExpanded = childNode?.expanded ?? false
            const childLoading = childNode?.loading ?? false
            return (
              <div key={d.path}>
                <div className="flex items-center group hover:bg-foreground/5 rounded-md transition-colors">
                  {/* 展开/折叠箭头 */}
                  <button
                    className="p-0.5 rounded hover:bg-foreground/10 transition-colors shrink-0"
                    onClick={() => toggleNode(d.path)}
                  >
                    {childLoading ? (
                      <div className="w-4 h-4 border-2 border-foreground/20 border-t-foreground/40 rounded-full animate-spin ml-1" />
                    ) : isExpanded ? (
                      <ChevronDown className="w-4 h-4 text-foreground/40" />
                    ) : (
                      <ChevronRight className="w-4 h-4 text-foreground/40" />
                    )}
                  </button>
                  {/* 目录图标 + 名称 */}
                  <button
                    className="flex-1 flex items-center gap-1.5 px-1 py-1.5 text-left min-w-0"
                    onClick={() => enterDir(d.path)}
                  >
                    {isExpanded ? (
                      <FolderOpen className="w-4 h-4 text-yellow-600/70 shrink-0" />
                    ) : (
                      <Folder className="w-4 h-4 text-yellow-600/60 shrink-0" />
                    )}
                    <span className="text-xs text-foreground/80 truncate">{d.name}</span>
                  </button>
                </div>
                {/* 已展开的子节点 */}
                {isExpanded && childNode?.loaded && (childNode.subdirs ?? []).map((cd) => (
                  <div key={cd.path} className="ml-5 flex items-center hover:bg-foreground/5 rounded-md transition-colors">
                    <button
                      className="flex-1 flex items-center gap-1.5 px-2 py-1 text-left min-w-0"
                      onClick={() => enterDir(cd.path)}
                    >
                      <Folder className="w-3.5 h-3.5 text-yellow-600/50 shrink-0" />
                      <span className="text-xs text-foreground/70 truncate">{cd.name}</span>
                    </button>
                  </div>
                ))}
              </div>
            )
          })}
        </div>

        {/* 错误提示 */}
        {error && (
          <div className="px-4 py-2 text-xs text-red-500 bg-red-50 dark:bg-red-950/20 border-t border-red-200 dark:border-red-900/30">
            {error}
          </div>
        )}

        {/* 底部操作栏 */}
        <div className="flex items-center gap-3 px-4 py-3 border-t border-border shrink-0 min-w-0">
          <div className="flex items-center gap-1 text-[10px] text-foreground/40 font-mono truncate min-w-0 flex-1" title={currentPath}>
            <HardDrive className="w-3 h-3 shrink-0" />
            <span className="truncate">{currentPath}</span>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <button
              className="px-3 py-1.5 text-xs rounded-lg border border-border text-foreground/60 hover:bg-foreground/5 transition-colors"
              onClick={onCancel}
            >
              取消
            </button>
            <button
              className="px-3 py-1.5 text-xs rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
              onClick={() => onSelect(currentPath)}
            >
              选择此目录
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
