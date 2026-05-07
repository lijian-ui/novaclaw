import { useState, useEffect, useCallback } from 'react'
import { ChevronRight, ChevronDown, Folder, FileCode, FileJson, FileType, FileImage, Loader2 } from 'lucide-react'

const API = 'http://127.0.0.1:3000/api/files'

interface FileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified: string
  extension: string
}

interface FileExplorerProps {
  onFileOpen?: (path: string, content: string) => void
}

export function FileExplorer({ onFileOpen }: FileExplorerProps) {
  const [currentPath, setCurrentPath] = useState('')
  const [entries, setEntries] = useState<FileEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set())

  const loadDir = useCallback(async (dirPath?: string) => {
    setLoading(true)
    try {
      const params = dirPath ? `?path=${encodeURIComponent(dirPath)}` : ''
      const res = await fetch(`${API}${params}`)
      if (res.ok) {
        const data = await res.json()
        setEntries(data.entries || [])
        setCurrentPath(data.current_path || data.current_dir || '')
      }
    } catch { /* ignore */ }
    setLoading(false)
  }, [])

  useEffect(() => { loadDir() }, [loadDir])

  const handleClick = useCallback(async (entry: FileEntry) => {
    if (entry.is_dir) {
      // Toggle expand
      if (expandedDirs.has(entry.path)) {
        setExpandedDirs(prev => { const n = new Set(prev); n.delete(entry.path); return n })
      } else {
        setExpandedDirs(prev => { const n = new Set(prev); n.add(entry.path); return n })
        loadDir(entry.path)
      }
    } else if (onFileOpen) {
      // Read file content
      try {
        const encoded = encodeURIComponent(entry.path)
        const res = await fetch(`http://127.0.0.1:3000/api/files/read?path=${encoded}`)
        if (res.ok) {
          const data = await res.json()
          onFileOpen(entry.path, data.content)
        }
      } catch {}
    }
  }, [onFileOpen, loadDir, expandedDirs])

  const getIcon = (entry: FileEntry) => {
    if (entry.is_dir) return <Folder className="w-3.5 h-3.5 text-amber-400 shrink-0" />
    const ext = (entry.extension || entry.name.split('.').pop() || '').toLowerCase()
    if (['ts', 'tsx', 'js', 'jsx'].includes(ext)) return <FileCode className="w-3.5 h-3.5 text-blue-400 shrink-0" />
    if (['json'].includes(ext)) return <FileJson className="w-3.5 h-3.5 text-yellow-400 shrink-0" />
    if (['css', 'scss', 'less'].includes(ext)) return <FileType className="w-3.5 h-3.5 text-purple-400 shrink-0" />
    if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'ico'].includes(ext)) return <FileImage className="w-3.5 h-3.5 text-green-400 shrink-0" />
    return <FileCode className="w-3.5 h-3.5 text-foreground/40 shrink-0" />
  }

  const formatSize = (size: number): string => {
    if (size > 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`
    if (size > 1024) return `${(size / 1024).toFixed(1)} KB`
    return `${size} B`
  }

  const parentDir = currentPath
    ? currentPath.substring(0, currentPath.lastIndexOf('\\', currentPath.length - 2))
    : ''

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center gap-2 px-4 py-4 border-b border-border shrink-0">
        <span className="text-sm font-semibold text-foreground/90 shrink-0">文件</span>
        {currentPath && (
          <span className="text-[10px] text-foreground/40 truncate font-mono" title={currentPath}>
            {currentPath}
          </span>
        )}
        {currentPath && (
          <button onClick={() => loadDir()} className="text-[10px] text-foreground/40 hover:text-foreground/60 transition-colors shrink-0 ml-auto">
            根目录
          </button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-2">
        {/* Parent directory */}
        {currentPath && currentPath.includes('workspace') && (
          <div
            className="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer hover:bg-foreground/5 transition-colors text-xs text-foreground/40"
            onClick={() => loadDir(parentDir)}
          >
            <ChevronRight className="w-3 h-3" />
            ..
          </div>
        )}

        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="w-4 h-4 animate-spin text-foreground/30" />
          </div>
        ) : entries.length === 0 ? (
          <div className="text-center py-8">
            <p className="text-xs text-foreground/30">空目录</p>
          </div>
        ) : (
          entries.map((entry) => (
            <div key={entry.path}>
              <div
                className="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer hover:bg-foreground/5 transition-colors group"
                onClick={() => handleClick(entry)}
              >
                {entry.is_dir ? (
                  expandedDirs.has(entry.path)
                    ? <ChevronDown className="w-3 h-3 text-foreground/30 shrink-0" />
                    : <ChevronRight className="w-3 h-3 text-foreground/30 shrink-0" />
                ) : (
                  <span className="w-3 shrink-0" />
                )}
                {getIcon(entry)}
                <span className="text-xs text-foreground/70 truncate flex-1">{entry.name}</span>
                {!entry.is_dir && (
                  <span className="text-[10px] text-foreground/30 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                    {formatSize(entry.size)}
                  </span>
                )}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
