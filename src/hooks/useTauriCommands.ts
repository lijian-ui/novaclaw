import { useCallback } from 'react'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const TauriApi = (window as any).__TAURI__

const isTauri = typeof TauriApi !== 'undefined' && TauriApi !== null

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    throw new Error('Not running in Tauri environment')
  }
  return TauriApi.invoke(command, args)
}

export function useTauriCommands() {
  /** 打开系统原生文件夹选择对话框 */
  const selectFolder = useCallback(async (): Promise<string> => {
    return invoke('select_folder')
  }, [])

  /** 设置开机自启动 */
  const setAutoStart = useCallback(async (enabled: boolean): Promise<void> => {
    await invoke('set_auto_start', { enabled })
  }, [])

  /** 清空本地缓存（已迁移至 REST POST /api/cache/clear，此接口保留为 Tauri 回退） */
  const clearLocalCache = useCallback(async (): Promise<void> => {
    await invoke('clear_local_cache')
  }, [])

  return {
    isTauri,
    selectFolder,
    setAutoStart,
    clearLocalCache,
  }
}
