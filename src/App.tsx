import { useState, useRef, useCallback, useEffect } from 'react'
import { Routes, Route } from 'react-router-dom'
import { Sidebar } from '@/components/Sidebar'
import { ChatPanel } from '@/components/ChatPanel'
import { Dashboard } from '@/pages/Dashboard'
import { FileExplorer } from '@/components/FileExplorer'
import { FileEditor } from '@/components/FileEditor'
import { useFileEditor } from '@/hooks/useFileEditor'

// 初始宽度比例：边栏10% / 聊天40% / 主控台40% / 文件22%
const INITIAL_CHAT_PERCENT = 0.40
const INITIAL_FILE_PERCENT = 0.15

function App() {
  // 全局禁用所有输入框的拼写检查（波浪线）
  useEffect(() => {
    const disableSpellcheck = () => {
      document.querySelectorAll('input, textarea').forEach(el => el.setAttribute('spellcheck', 'false'))
    }
    disableSpellcheck()
    const observer = new MutationObserver(disableSpellcheck)
    observer.observe(document.body, { childList: true, subtree: true })
    return () => observer.disconnect()
  }, [])

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [chatWidth, setChatWidth] = useState(() => Math.round(window.innerWidth * INITIAL_CHAT_PERCENT))
  const [fileWidth, setFileWidth] = useState(0)
  const [draggingTarget, setDraggingTarget] = useState<'chat' | 'file' | null>(null)
  const [activeTool, setActiveTool] = useState<string | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  const {
    tabs, activeTab, activePath, openFile, closeTab, updateContent, saveCurrent, switchTab,
  } = useFileEditor()

  const handleFileOpen = useCallback((path: string) => {
    openFile(path)
  }, [openFile])

  const handleEditorBack = useCallback(() => {
    if (activePath) closeTab(activePath)
  }, [activePath, closeTab])

  const openFilePanel = useCallback(() => {
    setFileWidth(Math.round(window.innerWidth * INITIAL_FILE_PERCENT))
  }, [])

  const toggleFilePanel = useCallback(() => {
    setFileWidth(prev => prev > 0 ? 0 : Math.round(window.innerWidth * INITIAL_FILE_PERCENT))
  }, [])

  const handleChatMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setDraggingTarget('chat')
  }, [])

  const handleFileMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setDraggingTarget('file')
  }, [])

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!draggingTarget || !containerRef.current) return
      const rect = containerRef.current.getBoundingClientRect()
      const x = e.clientX - rect.left

      if (draggingTarget === 'chat') {
        // 侧边栏固定宽度
        const sbWidth = sidebarCollapsed ? 50 : 220
        // 最大宽度 1300px
        const maxChatWidth = Math.min(rect.width - sbWidth - 50, 1300)
        const newChatWidth = Math.max(260, Math.min(maxChatWidth, x))
        setChatWidth(newChatWidth)

        // 当聊天面板向右挤压，右侧剩余空间不足时自动折叠文件面板
        const remainingRight = rect.width - sbWidth - newChatWidth
        if (fileWidth > 0 && remainingRight < 300) {
          setFileWidth(0)
        }
      } else {
        const totalWidth = rect.width
        const newWidth = totalWidth - x

        if (newWidth < 30) {
          setFileWidth(0)
          setDraggingTarget(null)
        } else {
          setFileWidth(Math.max(260, Math.min(600, newWidth)))
        }
      }
    },
    [draggingTarget, sidebarCollapsed, fileWidth]
  )

  const handleMouseUp = useCallback(() => {
    setDraggingTarget(null)
  }, [])

  // 全局阻止浏览器默认 Ctrl+S 行为
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault()
      }
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [])

  useEffect(() => {
    if (draggingTarget) {
      document.addEventListener('mousemove', handleMouseMove)
      document.addEventListener('mouseup', handleMouseUp)
      document.body.style.cursor = 'col-resize'
      document.body.style.userSelect = 'none'
    }
    return () => {
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [draggingTarget, handleMouseMove, handleMouseUp])

  return (
    <div ref={containerRef} className="h-screen w-screen flex overflow-hidden bg-mainbg">
      {/* 左侧任务列表（固定宽度，不受窗口拖动影响） */}
      <div
        className={`shrink-0 transition-all duration-200 ${
          sidebarCollapsed ? 'w-[50px] min-w-[50px] max-w-[50px]' : 'w-[220px] min-w-[220px] max-w-[220px]'
        }`}
      >
        <Sidebar collapsed={sidebarCollapsed} onToggle={() => setSidebarCollapsed(!sidebarCollapsed)} />
      </div>

      {/* Chat Area with draggable resize */}
      <div className="flex shrink-0" style={{ width: chatWidth }}>
        <div className="flex-1 min-w-0">
          <ChatPanel onOpenFilePanel={openFilePanel} onOpenTool={setActiveTool} />
        </div>
        <div
          className="w-1.5 cursor-col-resize hover:bg-foreground/5 active:bg-foreground/10 transition-colors shrink-0 relative"
          onMouseDown={handleChatMouseDown}
        >
          <div className="absolute inset-y-0 left-1/2 -translate-x-1/2 w-px bg-border" />
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 min-w-0">
        {activeTab ? (
          <FileEditor
            tabs={tabs}
            activeTab={activeTab}
            onUpdateContent={updateContent}
            onSave={saveCurrent}
            onCloseTab={closeTab}
            onSwitchTab={switchTab}
            onToggleFilePanel={toggleFilePanel}
          />
        ) : (
          <Routes>
            <Route path="/" element={<Dashboard activeTool={activeTool} onOpenTool={setActiveTool} onToggleFilePanel={toggleFilePanel} />} />
            <Route path="/dashboard" element={<Dashboard activeTool={activeTool} onOpenTool={setActiveTool} onToggleFilePanel={toggleFilePanel} />} />
          </Routes>
        )}
      </div>

      {/* File Preview with draggable resize */}
      <div className="flex shrink-0 overflow-hidden transition-all duration-300 ease-in-out" style={{ width: fileWidth }}>
        {fileWidth > 0 && (
          <>
            <div
              className="w-1.5 cursor-col-resize hover:bg-foreground/5 active:bg-foreground/10 transition-colors shrink-0 relative"
              onMouseDown={handleFileMouseDown}
            >
              <div className="absolute inset-y-0 left-1/2 -translate-x-1/2 w-px bg-border" />
            </div>
            <div className="flex-1 min-w-0">
              <FileExplorer onFileOpen={handleFileOpen} />
            </div>
          </>
        )}
      </div>
    </div>
  )
}

export default App
