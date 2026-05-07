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
  // 获取项目配置
  const getAppConfig = useCallback(async (): Promise<Record<string, unknown>> => {
    const jsonStr = await invoke<string>('get_config_json')
    return JSON.parse(jsonStr)
  }, [])

  // 保存项目配置
  const saveAppConfig = useCallback(async (config: Record<string, unknown>): Promise<void> => {
    const jsonStr = JSON.stringify(config)
    await invoke('save_config_json', { config_json: jsonStr })
  }, [])

  // 获取模型配置
  const getModelsConfig = useCallback(async (): Promise<Record<string, unknown>> => {
    const jsonStr = await invoke<string>('get_models_json')
    return JSON.parse(jsonStr)
  }, [])

  // 保存模型配置
  const saveModelsConfig = useCallback(async (config: Record<string, unknown>): Promise<void> => {
    const jsonStr = JSON.stringify(config)
    await invoke('save_models_json', { models_json: jsonStr })
  }, [])

  const setAutoStart = useCallback(async (enabled: boolean): Promise<void> => {
    await invoke('set_auto_start', { enabled })
  }, [])

  const clearLocalCache = useCallback(async (): Promise<void> => {
    await invoke('clear_local_cache')
  }, [])

  const selectFolder = useCallback(async (): Promise<string> => {
    return invoke('select_folder')
  }, [])

  const getDataDir = useCallback(async (): Promise<string> => {
    return invoke('get_data_dir')
  }, [])

  const getConfigDir = useCallback(async (): Promise<string> => {
    return invoke('get_config_dir')
  }, [])

  const getWorkspaceDir = useCallback(async (): Promise<string> => {
    return invoke('get_workspace_dir')
  }, [])

  const getSkillsDir = useCallback(async (): Promise<string> => {
    return invoke('get_skills_dir')
  }, [])

  const getMemoriesDir = useCallback(async (): Promise<string> => {
    return invoke('get_memories_dir')
  }, [])

  const getSessionsDir = useCallback(async (): Promise<string> => {
    return invoke('get_sessions_dir')
  }, [])

  return {
    isTauri,
    getAppConfig,
    saveAppConfig,
    getModelsConfig,
    saveModelsConfig,
    setAutoStart,
    clearLocalCache,
    selectFolder,
    getDataDir,
    getConfigDir,
    getWorkspaceDir,
    getSkillsDir,
    getMemoriesDir,
    getSessionsDir,
  }
}
