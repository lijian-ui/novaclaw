import { useEffect, useRef, useState, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { X, Terminal as TerminalIcon, Trash2, Maximize2, Minimize2, Plus } from 'lucide-react'
import { useTerminal } from '@/hooks/useTerminal'
import { useTheme } from '@/contexts/ThemeContext'
import { useTranslation } from 'react-i18next'
import { DEFAULT_TERMINAL_CONFIG, type TerminalConfig, type TerminalTab } from '@/types/terminal'
import '@xterm/xterm/css/xterm.css'

const MIN_HEIGHT = 120
const MAX_HEIGHT = 600
const DEFAULT_HEIGHT = 280

const terminalThemes = {
  dark: {
    background: '#1e1e1e',
    foreground: '#cccccc',
    cursor: '#cccccc',
    cursorAccent: '#1e1e1e',
    selectionBackground: '#264f78',
    selectionInactiveBackground: '#3a3d41',
    black: '#000000',
    red: '#cd3131',
    green: '#0dbc79',
    yellow: '#e5e510',
    blue: '#2472c8',
    magenta: '#bc3fbc',
    cyan: '#11a8cd',
    white: '#e5e5e5',
    brightBlack: '#666666',
    brightRed: '#f14c4c',
    brightGreen: '#23d18b',
    brightYellow: '#f5f543',
    brightBlue: '#3b8eea',
    brightMagenta: '#d670d6',
    brightCyan: '#29b8db',
    brightWhite: '#e5e5e5',
  },
  light: {
    background: '#ffffff',
    foreground: '#333333',
    cursor: '#333333',
    cursorAccent: '#ffffff',
    selectionBackground: '#add6ff',
    selectionInactiveBackground: '#e5e5e5',
    black: '#000000',
    red: '#cd3131',
    green: '#00bc00',
    yellow: '#bcbc00',
    blue: '#0000bc',
    magenta: '#bc00bc',
    cyan: '#00bcbc',
    white: '#e5e5e5',
    brightBlack: '#666666',
    brightRed: '#cd3131',
    brightGreen: '#00cd00',
    brightYellow: '#cdcd00',
    brightBlue: '#0000ee',
    brightMagenta: '#cd00cd',
    brightCyan: '#00cdcd',
    brightWhite: '#ffffff',
  },
}

interface TerminalPanelProps {
  visible: boolean
  onClose: () => void
}

function createTabId(): string {
  return `tab_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

export function TerminalPanel({ visible, onClose }: TerminalPanelProps) {
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const termContainerRef = useRef<HTMLDivElement>(null)
  const initializedRef = useRef(false)
  const lineBufferRef = useRef('')
  const historyRef = useRef<string[]>([])
  const historyIdxRef = useRef(-1)
  useTranslation()

  const [height, setHeight] = useState(DEFAULT_HEIGHT)
  const [maximized, setMaximized] = useState(false)
  const { theme: systemTheme } = useTheme()
  const terminalTheme = systemTheme === 'system' ? 'dark' : systemTheme
  const [config] = useState<TerminalConfig>(DEFAULT_TERMINAL_CONFIG)
  const [tabs, setTabs] = useState<TerminalTab[]>([
    { id: createTabId(), name: 'Terminal 1', sessionId: '', config: DEFAULT_TERMINAL_CONFIG },
  ])
  const [activeTabId, setActiveTabId] = useState(tabs[0]?.id || '')
  useState(false)

  const {
    connected,
    sendInput,
    sendCommand,
    killProcess,
    clearOutput,
    disconnect,
  } = useTerminal(terminalRef)

  const sendInputRef = useRef(sendInput)
  const sendCommandRef = useRef(sendCommand)
  useEffect(() => { sendInputRef.current = sendInput }, [sendInput])
  useEffect(() => { sendCommandRef.current = sendCommand }, [sendCommand])

  useEffect(() => {
    if (!visible || initializedRef.current) return
    initializedRef.current = true

    const fitAddon = new FitAddon()
    fitAddonRef.current = fitAddon

    const term = new Terminal({
      cursorBlink: config.cursorBlink,
      cursorStyle: config.cursorStyle,
      fontSize: config.fontSize,
      fontFamily: config.fontFamily,
      lineHeight: config.lineHeight,
      theme: terminalThemes[terminalTheme],
      allowTransparency: config.backgroundOpacity < 1.0,
      cols: 80,
      rows: 24,
      scrollback: config.scrollback,
      allowProposedApi: true,
    })

    term.loadAddon(fitAddon)
    term.loadAddon(new WebLinksAddon())
    term.open(termContainerRef.current!)

    /** VSCode 风格行输入模式：缓存输入，Enter 发送，↑↓ 历史，Ctrl+C 中断，Ctrl+L 清屏 */
    const disposeOnData = term.onData((data: string) => {
      if (data === '\x03') {
        // Ctrl+C: 立即发送到 shell，绕过输入缓冲
        sendInputRef.current('\x03')
        lineBufferRef.current = ''
        term.write('^C\r\n')
        return
      }

      for (const ch of data) {
        if (ch === '\r') {
          const cmd = lineBufferRef.current
          if (cmd.trim()) {
            historyRef.current.push(cmd)
            historyIdxRef.current = historyRef.current.length
          }
          lineBufferRef.current = ''
          term.write('\r\n')
          sendCommandRef.current(cmd)
        } else if (ch === '\x7f' || ch === '\b') {
          if (lineBufferRef.current.length > 0) {
            lineBufferRef.current = lineBufferRef.current.slice(0, -1)
            term.write('\b \b')
          }
        } else if (ch === '\x0c') {
          // Ctrl+L: 清屏
          term.write('\x0c')
        } else if (ch === '\x1b') {
          // ESC 清除当前行
          if (lineBufferRef.current.length > 0) {
            term.write('\b \b'.repeat(lineBufferRef.current.length))
            lineBufferRef.current = ''
          }
        } else if (ch >= ' ') {
          lineBufferRef.current += ch
          term.write(ch)
        }
      }
    })

    /** 处理上下箭头历史导航（VT100 序列） */
    const disposeOnEscape = term.onData((data: string) => {
      if (data === '\x1b[A') {
        // 上箭头：显示上一条历史
        if (historyRef.current.length > 0 && historyIdxRef.current > 0) {
          historyIdxRef.current--
          // 清除当前行
          if (lineBufferRef.current.length > 0) {
            term.write('\b \b'.repeat(lineBufferRef.current.length))
          }
          const cmd = historyRef.current[historyIdxRef.current]
          lineBufferRef.current = cmd
          term.write(cmd)
        }
      } else if (data === '\x1b[B') {
        // 下箭头：显示下一条历史
        if (historyIdxRef.current < historyRef.current.length - 1) {
          historyIdxRef.current++
          if (lineBufferRef.current.length > 0) {
            term.write('\b \b'.repeat(lineBufferRef.current.length))
          }
          const cmd = historyRef.current[historyIdxRef.current]
          lineBufferRef.current = cmd
          term.write(cmd)
        } else {
          historyIdxRef.current = historyRef.current.length
          if (lineBufferRef.current.length > 0) {
            term.write('\b \b'.repeat(lineBufferRef.current.length))
          }
          lineBufferRef.current = ''
        }
      }
    })

    terminalRef.current = term

    /** 右键复制/粘贴：选择文本时右键复制，空白处右键粘贴 */
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault()
      if (term.hasSelection()) {
        // 有选中文本 → 复制，复制后取消选中
        const selected = term.getSelection()
        navigator.clipboard.writeText(selected).then(() => {
          term.clearSelection()
        }).catch(() => {
          // 降级方案：通过 textarea 复制
          const textarea = document.createElement('textarea')
          textarea.value = selected
          textarea.style.position = 'fixed'
          textarea.style.opacity = '0'
          document.body.appendChild(textarea)
          textarea.select()
          document.execCommand('copy')
          document.body.removeChild(textarea)
          term.clearSelection()
        })
        term.focus()
      } else {
        // 无选中文本 → 粘贴
        navigator.clipboard.readText().then(text => {
          if (text) {
            sendInputRef.current(text)
          }
        }).catch(() => {
          // 降级方案：通过 prompt 获取粘贴内容
          const pasted = prompt('Paste:')
          if (pasted) {
            sendInputRef.current(pasted)
          }
        })
        term.focus()
      }
    }
    const container = termContainerRef.current
    container?.addEventListener('contextmenu', handleContextMenu)

    const fitTimer = setTimeout(() => fitAddon.fit(), 100)
    const handleResize = () => fitAddon.fit()
    window.addEventListener('resize', handleResize)

    const ro = new ResizeObserver(() => fitAddon.fit())
    if (termContainerRef.current) {
      ro.observe(termContainerRef.current)
    }

    return () => {
      clearTimeout(fitTimer)
      disposeOnData.dispose()
      disposeOnEscape.dispose()
      container?.removeEventListener('contextmenu', handleContextMenu)
      window.removeEventListener('resize', handleResize)
      ro.disconnect()
      term.dispose()
      terminalRef.current = null
      fitAddonRef.current = null
      initializedRef.current = false
    }
  }, [visible, terminalTheme, config])

  useEffect(() => {
    if (!visible) return
    const t = setTimeout(() => fitAddonRef.current?.fit(), 80)
    return () => clearTimeout(t)
  }, [height, maximized, visible])

  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.options.theme = terminalThemes[terminalTheme]
    }
  }, [terminalTheme])

  /** 添加新标签 */
  const addTab = useCallback(() => {
    const newTab: TerminalTab = {
      id: createTabId(),
      name: `Terminal ${tabs.length + 1}`,
      sessionId: '',
      config: { ...DEFAULT_TERMINAL_CONFIG },
    }
    setTabs(prev => [...prev, newTab])
    setActiveTabId(newTab.id)
  }, [tabs.length])

  /** 关闭标签 */
  const closeTab = useCallback((tabId: string) => {
    setTabs(prev => {
      const idx = prev.findIndex(t => t.id === tabId)
      const filtered = prev.filter(t => t.id !== tabId)
      if (filtered.length === 0) {
        const newTab: TerminalTab = {
          id: createTabId(),
          name: 'Terminal 1',
          sessionId: '',
          config: { ...DEFAULT_TERMINAL_CONFIG },
        }
        return [newTab]
      }
      if (activeTabId === tabId) {
        const newIdx = Math.min(idx, filtered.length - 1)
        setActiveTabId(filtered[newIdx].id)
      }
      return filtered
    })
  }, [activeTabId])

  const handleClosePanel = useCallback(() => {
    disconnect()
    onClose()
  }, [disconnect, onClose])

  const toggleMaximize = useCallback(() => {
    setMaximized(prev => !prev)
  }, [])

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    const startY = e.clientY
    const startH = height
    document.body.style.cursor = 'row-resize'
    document.body.style.userSelect = 'none'
    const onMove = (ev: MouseEvent) => {
      const diff = startY - ev.clientY
      setHeight(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, startH + diff)))
    }
    const onUp = () => {
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      setTimeout(() => fitAddonRef.current?.fit(), 50)
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }, [height])

  if (!visible) return null

  return (
    <div
      className="shrink-0 border-t border-border flex flex-col"
      style={{
        height: maximized ? '50vh' : height,
        background: terminalTheme === 'dark' ? '#1e1e1e' : '#ffffff',
      }}
    >
      {/* Resize Handle */}
      <div className="h-[3px] cursor-row-resize shrink-0 relative" onMouseDown={handleMouseDown}>
        <div className="absolute inset-x-0 top-0 h-[1px] bg-[#007acc]/30 hover:bg-[#007acc]/70 transition-colors" />
      </div>

      {/* Tab Bar */}
      <div
        className="flex items-center shrink-0 h-[35px] px-2 gap-0"
        style={{ background: terminalTheme === 'dark' ? '#252526' : '#f5f5f5', borderBottom: '1px solid var(--border)' }}
      >
        <div className="flex items-center flex-1 overflow-x-auto gap-0">
          {tabs.map(tab => (
            <div
              key={tab.id}
              onClick={() => setActiveTabId(tab.id)}
              className={`flex items-center gap-1 px-3 h-full cursor-pointer text-xs select-none shrink-0 border-r transition-colors ${
                tab.id === activeTabId
                  ? terminalTheme === 'dark'
                    ? 'bg-[#1e1e1e] text-[#cccccc]'
                    : 'bg-[#ffffff] text-[#333333]'
                  : terminalTheme === 'dark'
                    ? 'bg-[#2d2d2d] text-[#666666] hover:text-[#999999]'
                    : 'bg-[#eaeaea] text-[#999999] hover:text-[#666666]'
              }`}
            >
              <TerminalIcon className="w-3 h-3 shrink-0" />
              <span className="truncate max-w-[120px]">{tab.name}</span>
              {tabs.length > 1 && (
                <button
                  onClick={(e) => { e.stopPropagation(); closeTab(tab.id) }}
                  className="ml-0.5 p-0.5 rounded hover:bg-foreground/10 transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </div>
          ))}
          <button
            onClick={addTab}
            className="flex items-center justify-center w-[28px] h-full shrink-0 hover:bg-foreground/10 transition-colors"
            style={{ color: terminalTheme === 'dark' ? '#666666' : '#999999' }}
            title="New Terminal"
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
        </div>

        {/* Title Bar Actions */}
        <div className="flex items-center gap-0.5 shrink-0 ml-2">
          <button
            title="Kill process"
            onClick={killProcess}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ color: terminalTheme === 'dark' ? '#999999' : '#666666' }}
            onMouseEnter={(e) => { e.currentTarget.style.background = terminalTheme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)' }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent' }}
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
          <button
            title="Clear"
            onClick={clearOutput}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ color: terminalTheme === 'dark' ? '#999999' : '#666666' }}
            onMouseEnter={(e) => { e.currentTarget.style.background = terminalTheme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)' }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent' }}
          >
            <TerminalIcon className="w-3.5 h-3.5" />
          </button>
          <button
            title={maximized ? 'Minimize' : 'Maximize'}
            onClick={toggleMaximize}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ color: terminalTheme === 'dark' ? '#999999' : '#666666' }}
            onMouseEnter={(e) => { e.currentTarget.style.background = terminalTheme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)' }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent' }}
          >
            {maximized ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
          </button>
          {/* Connection Status Indicator */}
          <div className={`w-[6px] h-[6px] rounded-full mx-1 ${connected ? 'bg-green-400' : 'bg-red-400'}`} />
          <button
            title="Close terminal"
            onClick={handleClosePanel}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ color: terminalTheme === 'dark' ? '#999999' : '#666666' }}
            onMouseEnter={(e) => { e.currentTarget.style.background = terminalTheme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)' }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent' }}
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Terminal Container */}
      <div
        ref={termContainerRef}
        className="flex-1 overflow-hidden p-[4px]"
        style={{
          background: terminalThemes[terminalTheme].background,
          opacity: config.backgroundOpacity,
        }}
      />
    </div>
  )
}
