/** WebSocket 终端协议消息类型 */
export interface TerminalMessage {
  type: 'stdout' | 'stderr' | 'exit' | 'error' | 'clear'
  data?: string
  code?: number
}

/** 终端输出行 */
export interface TerminalLine {
  /** 行内容 */
  text: string
  /** 类型: normal / error / system */
  kind: 'normal' | 'error' | 'system'
  /** 时间戳 */
  timestamp: number
}

/** useTerminal Hook 返回值 */
export interface UseTerminalReturn {
  /** 终端输出行列表 */
  lines: TerminalLine[]
  /** 是否已连接 */
  connected: boolean
  /** 是否正在运行命令 */
  running: boolean
  /** 连接错误信息 */
  error: string | null
  /** 命令历史 */
  history: string[]
  /** 当前历史索引（-1 表示新输入） */
  historyIndex: number
  /** 发送命令 */
  sendCommand: (cmd: string) => void
  /** 终止当前进程 */
  killProcess: () => void
  /** 清屏 */
  clearOutput: () => void
  /** 设置历史索引 */
  setHistoryIndex: (idx: number) => void
  /** 断开连接 */
  disconnect: () => void
}
