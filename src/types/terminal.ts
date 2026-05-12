/** WebSocket 终端协议消息类型 */
export interface TerminalMessage {
  type: 'stdout' | 'stderr' | 'exit' | 'error' | 'clear' | 'session_restarted'
  data?: string
  code?: number
  session_id?: string
}

/** 终端配置选项 */
export interface TerminalConfig {
  fontSize: number
  lineHeight: number
  fontFamily: string
  cursorBlink: boolean
  cursorStyle: 'block' | 'underline' | 'bar'
  backgroundOpacity: number
  scrollback: number
}

/** 默认终端配置 */
export const DEFAULT_TERMINAL_CONFIG: TerminalConfig = {
  fontSize: 13,
  lineHeight: 1.35,
  fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', 'Courier New', monospace",
  cursorBlink: true,
  cursorStyle: 'bar',
  backgroundOpacity: 1.0,
  scrollback: 10000,
}

/** 终端标签页 */
export interface TerminalTab {
  id: string
  name: string
  sessionId: string
  config: TerminalConfig
}

/** useTerminal Hook 返回值 */
export interface UseTerminalReturn {
  connected: boolean
  running: boolean
  error: string | null
  sendInput: (data: string) => void
  sendCommand: (cmd: string) => void
  killProcess: () => void
  clearOutput: () => void
  resize: (cols: number, rows: number) => void
  disconnect: () => void
  reconnect: () => void
}
