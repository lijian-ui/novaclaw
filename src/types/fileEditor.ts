/** 文件条目 */
export interface FileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified: string
  extension: string
}

/** 已打开的文件编辑页签 */
export interface EditorTab {
  /** 文件路径 */
  path: string
  /** 文件名 */
  name: string
  /** 当前内容（编辑中） */
  content: string
  /** 初始内容（加载时） */
  initialContent: string
  /** 是否有未保存更改 */
  dirty: boolean
  /** 语言类型 */
  language: string
}

/** 文件变更事件 */
export interface FileChangeEvent {
  type: 'changed' | 'deleted'
  path: string
  content: string
}

/** useFileEditor Hook 返回值 */
export interface UseFileEditorReturn {
  /** 已打开的页签列表 */
  tabs: EditorTab[]
  /** 当前激活的页签路径 */
  activePath: string | null
  /** 当前激活的页签 */
  activeTab: EditorTab | null
  /** 打开文件 */
  openFile: (path: string, initialContent?: string) => Promise<void>
  /** 关闭页签 */
  closeTab: (path: string) => void
  /** 更新当前文件内容 */
  updateContent: (content: string) => void
  /** 保存当前文件 */
  saveCurrent: () => Promise<void>
  /** 切换到指定页签 */
  switchTab: (path: string) => void
  /** 连接状态 */
  connected: boolean
  /** WebSocket 连接错误 */
  error: string | null
}
