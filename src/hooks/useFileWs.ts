/**
 * 共享文件 WebSocket 单例
 * FileExplorer 和 useFileEditor 共用同一个连接
 */

const WS_URL = 'ws://127.0.0.1:3000/ws/files'

type WsListener = (msg: Record<string, unknown>) => void

let sharedWs: WebSocket | null = null
let connectPromise: Promise<WebSocket> | null = null
let listeners: WsListener[] = []

function notifyListeners(msg: Record<string, unknown>) {
  for (const fn of listeners) {
    try { fn(msg) } catch { /* ignore */ }
  }
}

/** 获取或创建共享 WebSocket 连接 */
export function getFileWebSocket(): Promise<WebSocket> {
  if (sharedWs?.readyState === WebSocket.OPEN) {
    return Promise.resolve(sharedWs)
  }
  if (connectPromise) return connectPromise

  connectPromise = new Promise((resolve, reject) => {
    const ws = new WebSocket(WS_URL)
    let resolved = false

    ws.onopen = () => {
      sharedWs = ws
      resolved = true
      resolve(ws)
    }

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data)
        notifyListeners(msg)
      } catch { /* ignore */ }
    }

    ws.onerror = () => {
      sharedWs = null
      connectPromise = null
      if (!resolved) reject(new Error('WebSocket 连接失败'))
    }

    ws.onclose = () => {
      sharedWs = null
      connectPromise = null
    }
  })

  return connectPromise
}

/** 注册消息监听器，返回取消注册函数 */
export function onFileWsMessage(fn: WsListener): () => void {
  listeners.push(fn)
  return () => {
    listeners = listeners.filter(l => l !== fn)
  }
}

/** 发送消息到共享 WebSocket */
export async function sendFileWs(data: Record<string, unknown>): Promise<void> {
  const ws = await getFileWebSocket()
  ws.send(JSON.stringify(data))
}
