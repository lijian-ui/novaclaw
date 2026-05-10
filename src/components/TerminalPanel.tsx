/**
 * TerminalPanel — xterm.js 终端面板（VS Code 风格）
 *
 * 行输入模式：前端缓存用户输入，按 Enter 后发送完整命令到后端。
 * 这种方式适配 Windows cmd.exe 管道模式，命令能够正确执行并返回输出。
 *
 * 未来可升级为 ConPTY 伪终端模式以支持完全交互式体验。
 */

import { useEffect, useRef, useState, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { X, Terminal as TerminalIcon, Trash2, Maximize2, Minimize2 } from 'lucide-react'
import { useTerminal } from '@/hooks/useTerminal'
import { useTheme } from '@/contexts/ThemeContext'
import { useTranslation } from 'react-i18next'
import '@xterm/xterm/css/xterm.css'

const MIN_HEIGHT = 120
const MAX_HEIGHT = 600
const DEFAULT_HEIGHT = 280

// 终端主题配置
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

export function TerminalPanel({ visible, onClose }: TerminalPanelProps) {
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const termContainerRef = useRef<HTMLDivElement>(null)
  const initializedRef = useRef(false)
  const lineBufferRef = useRef('')
  const { t } = useTranslation()

  const [height, setHeight] = useState(DEFAULT_HEIGHT)
  const [maximized, setMaximized] = useState(false)
  const { theme } = useTheme()

  // 终端通信钩子（自动连接）
  const {
    connected,
    error,
    sendInput,
    sendCommand,
    killProcess,
    clearOutput,
    disconnect,
  } = useTerminal(terminalRef)

  // ---- 初始化 xterm.js ----
  useEffect(() => {
    if (!visible || initializedRef.current) return
    initializedRef.current = true

    const fitAddon = new FitAddon()
    fitAddonRef.current = fitAddon

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      fontSize: 13,
      fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', 'Courier New', monospace",
      lineHeight: 1.35,
      theme: terminalThemes[theme],
      allowTransparency: false,
      cols: 80,
      rows: 24,
      scrollback: 10000,
      allowProposedApi: true,
    })

    term.loadAddon(fitAddon)

    // 挂载到 DOM
    term.open(termContainerRef.current!)

    // ---- 行输入模式 ----
    // 前端缓存用户输入，按 Enter 后发送完整命令到后端。
    // 使用 \r\n 换行（Windows cmd.exe 兼容）
    const disposeOnData = term.onData((data: string) => {
      for (const ch of data) {
        if (ch === '\r') {
          // 回车：执行当前行
          const cmd = lineBufferRef.current
          lineBufferRef.current = ''
          term.write('\r\n')
          if (cmd.trim()) {
            sendCommand(cmd)
          } else {
            // 空命令也发送，后端会显示提示符
            sendCommand('')
          }
        } else if (ch === '\x7f' || ch === '\b') {
          // 退格
          if (lineBufferRef.current.length > 0) {
            lineBufferRef.current = lineBufferRef.current.slice(0, -1)
            term.write('\b \b')
          }
        } else if (ch === '\x03') {
          // Ctrl+C
          lineBufferRef.current = ''
          term.write('^C\r\n')
          sendCommand('')
        } else if (ch === '\x1b') {
          // ESC 键 - 清除当前行
          if (lineBufferRef.current.length > 0) {
            term.write('\b \b'.repeat(lineBufferRef.current.length))
            lineBufferRef.current = ''
          }
        } else if (ch >= ' ') {
          // 可打印字符
          lineBufferRef.current += ch
          term.write(ch)
        }
      }
    })

    terminalRef.current = term

    // 适配尺寸
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
      window.removeEventListener('resize', handleResize)
      ro.disconnect()
      term.dispose()
      terminalRef.current = null
      fitAddonRef.current = null
      initializedRef.current = false
    }
  }, [visible, sendCommand])

  // ---- 面板高度变化后重新 fit ----
  useEffect(() => {
    if (!visible) return
    const t = setTimeout(() => fitAddonRef.current?.fit(), 80)
    return () => clearTimeout(t)
  }, [height, maximized, visible])

  // ---- 主题变化时更新终端样式 ----
  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.options.theme = terminalThemes[theme]
    }
  }, [theme])

  const handleClosePanel = useCallback(() => {
    disconnect()
    onClose()
  }, [disconnect, onClose])

  const toggleMaximize = useCallback(() => {
    setMaximized((prev) => !prev)
  }, [])

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
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
    },
    [height],
  )

  if (!visible) return null

  return (
    <div
      className="shrink-0 border-t border-border flex flex-col"
      style={{
        height: maximized ? '50vh' : height,
        background: theme === 'dark' ? '#1e1e1e' : '#ffffff',
      }}
    >
      <div
        className="h-[3px] cursor-row-resize shrink-0 relative"
        onMouseDown={handleMouseDown}
      >
        <div className="absolute inset-x-0 top-0 h-[1px] bg-[#007acc]/30 hover:bg-[#007acc]/70 transition-colors" />
      </div>

      {/* 标题栏 */}
      <div 
        className="flex items-center justify-between h-[32px] px-2 shrink-0 select-none"
        style={{ background: theme === 'dark' ? '#252526' : '#f5f5f5' }}
      >
        <div className="flex items-center gap-2">
          <TerminalIcon className="w-3.5 h-3.5" style={{ color: theme === 'dark' ? '#cccccc' : '#333333' }} />
          <span 
            className="text-xs font-medium tracking-wide"
            style={{ color: theme === 'dark' ? '#cccccc' : '#333333' }}
          >
            {t('terminal.title')}
          </span>
          <div
            className={`w-[6px] h-[6px] rounded-full ${
              connected ? 'bg-[#89d185]' : 'bg-[#f14c4c]'
            }`}
          />
          {connected && (
            <span 
              className="text-[10px]"
              style={{ color: theme === 'dark' ? 'rgba(137, 209, 133, 0.6)' : 'rgba(137, 209, 133, 0.8)' }}
            >
              {t('terminal.connected')}
            </span>
          )}
          {!connected && !error && (
            <span 
              className="text-[10px]"
              style={{ color: theme === 'dark' ? 'rgba(241, 76, 76, 0.6)' : 'rgba(241, 76, 76, 0.8)' }}
            >
              {t('terminal.disconnected')}
            </span>
          )}
          {error && (
            <span className="text-[10px] text-[#f14c4c]">{error}</span>
          )}
        </div>
        <div className="flex items-center gap-0.5">
          <button
            title={t('terminal.kill')}
            onClick={killProcess}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ 
              background: 'transparent',
              color: theme === 'dark' ? '#999999' : '#666666'
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = theme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'transparent';
            }}
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
          <button
            title={t('terminal.clear')}
            onClick={clearOutput}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ 
              background: 'transparent',
              color: theme === 'dark' ? '#999999' : '#666666'
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = theme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'transparent';
            }}
          >
            <TerminalIcon className="w-3.5 h-3.5" />
          </button>
          <button
            title={maximized ? t('terminal.minimize') : t('terminal.maximize')}
            onClick={toggleMaximize}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ 
              background: 'transparent',
              color: theme === 'dark' ? '#999999' : '#666666'
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = theme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'transparent';
            }}
          >
            {maximized ? (
              <Minimize2 className="w-3.5 h-3.5" />
            ) : (
              <Maximize2 className="w-3.5 h-3.5" />
            )}
          </button>
          <button
            title={t('terminal.close')}
            onClick={handleClosePanel}
            className="w-[26px] h-[26px] flex items-center justify-center rounded transition-colors"
            style={{ 
              background: 'transparent',
              color: theme === 'dark' ? '#999999' : '#666666'
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = theme === 'dark' ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'transparent';
            }}
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* 终端容器 */}
      <div
        ref={termContainerRef}
        className="flex-1 overflow-hidden p-[4px]"
        style={{ background: theme === 'dark' ? '#1e1e1e' : '#ffffff' }}
      />
    </div>
  )
}
