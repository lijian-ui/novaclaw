/** WebSocket 终端协议消息类型 */
export interface TerminalMessage {
  type: 'stdout' | 'stderr' | 'exit' | 'error' | 'clear'
  data?: string
  code?: number
}

/** useTerminal Hook 返回值 */
export interface UseTerminalReturn {
  /** 是否已连接 */
  connected: boolean
  /** 是否正在运行命令 */
  running: boolean
  /** 连接错误 */
  error: string | null
  /** 发送用户输入（xterm onData 触发） */
  sendInput: (data: string) => void
  /** 执行单条命令 */
  sendCommand: (cmd: string) => void
  /** 终止当前进程 */
  killProcess: () => void
  /** 清屏 */
  clearOutput: () => void
  /** 调整终端尺寸 */
  resize: (cols: number, rows: number) => void
  /** 断开连接 */
  disconnect: () => void
  /** 重新连接 */
  reconnect: () => void
}
