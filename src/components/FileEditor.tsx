import { useState, useMemo, useRef, useCallback, useEffect } from 'react'
import { X, Save, PanelRightClose } from 'lucide-react'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { vscDarkPlus, oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import mermaid from 'mermaid'
import { useTheme } from '@/contexts/ThemeContext'
import type { EditorTab } from '@/types/fileEditor'

mermaid.initialize({
  startOnLoad: false,
  securityLevel: 'loose',
  fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
})

/** react-syntax-highlighter 语言映射 */
const langMap: Record<string, string> = {
  typescript: 'typescript', javascript: 'javascript', jsx: 'jsx', tsx: 'tsx',
  html: 'markup', htm: 'markup', css: 'css', scss: 'scss', less: 'less',
  json: 'json', markdown: 'markdown', md: 'markdown',
  rust: 'rust', python: 'python', go: 'go', java: 'java',
  bash: 'bash', sh: 'bash', zsh: 'bash',
  yaml: 'yaml', yml: 'yaml', toml: 'toml', sql: 'sql',
  xml: 'xml', svg: 'xml', php: 'php', ruby: 'ruby',
}

interface FileEditorProps {
  tabs: EditorTab[]
  activeTab: EditorTab | null
  onUpdateContent: (content: string) => void
  onSave: () => void
  onCloseTab: (path: string) => void
  onSwitchTab: (path: string) => void
  onToggleFilePanel?: () => void
}

// ---- 语法高亮（由 react-syntax-highlighter 接管） ----

/** 将语言名转为 react-syntax-highlighter 支持的语言 ID */
function getHighlightLang(lang: string): string {
  return langMap[lang] || lang
}

/** Markdown 实时预览（使用 react-markdown + mermaid） */
function MarkdownPreview({ content }: { content: string }) {
  const { isLight } = useTheme()
  const containerRef = useRef<HTMLDivElement>(null)

  // 渲染后自动渲染 Mermaid 图表（根据当前明暗主题切换主题色）
  useEffect(() => {
    mermaid.initialize({
      startOnLoad: false,
      theme: isLight ? 'default' : 'dark',
      securityLevel: 'loose',
      fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
    })
    mermaid.run()
  }, [isLight])

  return (
    <div ref={containerRef} className="markdown-preview p-4 text-sm leading-relaxed overflow-y-auto h-full">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || '')
            const codeStr = String(children).replace(/\n$/, '')
            if (match) {
              // Mermaid 图表
              if (match[1] === 'mermaid') {
                return <pre className="mermaid">{codeStr}</pre>
              }
              return (
                <SyntaxHighlighter
                  language={match[1]}
                  style={isLight ? oneLight : vscDarkPlus}
                  customStyle={{
                    margin: 0,
                    padding: '12px',
                    fontSize: 13,
                    borderRadius: 6,
                    fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', 'Courier New', monospace",
                  }}
                >
                  {codeStr}
                </SyntaxHighlighter>
              )
            }
            return (
              <code className="hl-inline-code" {...props}>
                {children}
              </code>
            )
          },
          a({ href, children }) {
            return (
              <a href={href} target="_blank" rel="noopener noreferrer">
                {children}
              </a>
            )
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
}

export function FileEditor({ tabs, activeTab, onUpdateContent, onSave, onCloseTab, onSwitchTab, onToggleFilePanel }: FileEditorProps) {
  const { isLight } = useTheme()
  const content = activeTab?.content || ''
  const language = activeTab?.language || ''
  const fileName = activeTab?.name || ''
  const dirty = activeTab?.dirty || false
  const isMarkdown = language === 'markdown'
  const [preview, setPreview] = useState(isMarkdown)

  const hlLang = useMemo(() => getHighlightLang(language), [language])
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const hlContainerRef = useRef<HTMLDivElement>(null)
  const gutterRef = useRef<HTMLDivElement>(null)

  const lineCount = content.split('\n').length
  const lineNumArr = useMemo(() => Array.from({ length: lineCount }, (_, i) => i + 1), [lineCount])

  const syncScroll = useCallback(() => {
    if (textareaRef.current && hlContainerRef.current) {
      hlContainerRef.current.scrollTop = textareaRef.current.scrollTop
      hlContainerRef.current.scrollLeft = textareaRef.current.scrollLeft
    }
    if (gutterRef.current && textareaRef.current) {
      gutterRef.current.scrollTop = textareaRef.current.scrollTop
    }
  }, [])

  // 编辑内容变化时触发自动保存（防抖在 hook 中处理）
  const handleContentChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    onUpdateContent(e.target.value)
  }, [onUpdateContent])

  // 切换到 markdown 时默认开启预览
  useEffect(() => {
    if (isMarkdown) setPreview(true)
  }, [isMarkdown])

  if (!activeTab) return null

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Tabs header */}
      <div className="flex items-center border-b border-border shrink-0 overflow-x-auto">
        {tabs.map(tab => (
          <div
            key={tab.path}
            onClick={() => onSwitchTab(tab.path)}
            onMouseDown={(e) => { if (e.button === 1) { e.preventDefault(); onCloseTab(tab.path); } }}
            className={`group flex items-center gap-1.5 px-3 py-2 cursor-pointer border-r border-border shrink-0 text-xs transition-colors ${
              tab.path === activeTab.path
                ? 'bg-mainbg text-foreground/90 border-b-2 border-b-blue-500'
                : 'bg-foreground/[0.02] text-foreground/50 hover:text-foreground/70'
            }`}
          >
            <span className="truncate max-w-[120px]">{tab.name}</span>
            {tab.dirty && <span className="w-1.5 h-1.5 rounded-full bg-amber-400 shrink-0" />}
            <button
              onClick={e => { e.stopPropagation(); onCloseTab(tab.path) }}
              className="ml-1 p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-foreground/10 transition-all"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        ))}
      </div>

      {/* File actions bar */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-border shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-xs font-medium text-foreground/80 truncate">{fileName}</span>
          {dirty && <span className="text-[10px] text-amber-400/80">未保存</span>}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {isMarkdown && (
            <button
              onClick={() => setPreview(p => !p)}
              className={`px-2 py-1 text-xs rounded transition-colors ${preview ? 'text-foreground/50 hover:text-foreground/70 hover:bg-foreground/10' : 'bg-blue-500/20 text-blue-400'}`}
            >
              {preview ? '编辑' : '预览'}
            </button>
          )}
          {onToggleFilePanel && (
            <button
              onClick={onToggleFilePanel}
              className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
              title="切换文件预览面板"
            >
              <PanelRightClose className="w-4 h-4 text-foreground/50" />
            </button>
          )}
          <button
            onClick={onSave}
            disabled={!dirty}
            className="flex items-center gap-1 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:bg-blue-500/40 text-xs text-white font-medium transition-colors"
          >
            <Save className="w-3.5 h-3.5" />
            保存
          </button>
        </div>
      </div>

      {/* Editor / Preview 单栏切换 */}
      <div className="flex-1 overflow-hidden">
        {/* 编辑模式 */}
        {!preview && (
          <div className="flex flex-row h-full">
            {/* Line number gutter */}
            <div
              ref={gutterRef}
              className="overflow-hidden select-none shrink-0 border-r border-border/50"
              style={{ width: '3.75em', minWidth: '3.75em' }}
            >
              <div className="py-4 font-mono text-sm leading-relaxed text-right text-foreground/30">
                {lineNumArr.map(num => (
                  <div key={num} className="px-2 text-sm leading-relaxed">{num}</div>
                ))}
              </div>
            </div>

            {/* Code area */}
            <div className="relative flex-1 min-w-0">
              <div ref={hlContainerRef} className="hl-container absolute inset-0 overflow-hidden pointer-events-none" aria-hidden="true">
                <SyntaxHighlighter
                  language={hlLang}
                  style={isLight ? oneLight : vscDarkPlus}
                  customStyle={{
                    margin: 0,
                    padding: '16px',
                    fontSize: 13,
                    lineHeight: '1.65',
                    fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', 'Courier New', monospace",
                    background: 'transparent',
                    overflow: 'visible',
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-word',
                    overflowWrap: 'break-word',
                  }}
                  PreTag="div"
                  showLineNumbers={false}
                >
                  {content || ' '}
                </SyntaxHighlighter>
              </div>
              <textarea
                ref={textareaRef}
                value={content}
                onChange={handleContentChange}
                onScroll={syncScroll}
                className="file-editor-textarea absolute inset-0 w-full h-full px-4 py-4 font-mono bg-transparent text-transparent caret-foreground outline-none resize-none whitespace-pre-wrap overflow-auto"
                spellCheck={false}
                style={{
                  fontSize: 13,
                  lineHeight: '1.65',
                  fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', 'Courier New', monospace",
                }}
              />
            </div>
          </div>
        )}

        {/* 预览模式（仅 Markdown） */}
        {preview && isMarkdown && (
          <MarkdownPreview content={content} />
        )}
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between px-4 py-1.5 border-t border-border bg-foreground/[0.02] shrink-0">
        <span className="text-[10px] text-foreground/40 font-mono">{language.toUpperCase()}</span>
        <span className="text-[10px] text-foreground/40 font-mono">
          {content.split('\n').length} 行 | {content.length} 字符
        </span>
      </div>

      {/* 文本域选中样式 + SyntaxHighlighter 自动换行 + Markdown 预览样式 */}
      <style>{`
        .file-editor-textarea::selection {
          background: rgba(56, 139, 253, 0.3);
        }
        .file-editor-textarea::-moz-selection {
          background: rgba(56, 139, 253, 0.3);
        }
        .hl-container code {
          white-space: pre-wrap !important;
          word-break: break-word !important;
          overflow-wrap: break-word !important;
        }

        /* Markdown 预览样式 */
        .markdown-preview h1 { font-size: 1.75rem; font-weight: 700; margin: 1.5rem 0 0.75rem; border-bottom: 1px solid hsl(var(--border)); padding-bottom: 0.3rem; }
        .markdown-preview h2 { font-size: 1.4rem; font-weight: 600; margin: 1.25rem 0 0.6rem; border-bottom: 1px solid hsl(var(--border)); padding-bottom: 0.2rem; }
        .markdown-preview h3 { font-size: 1.15rem; font-weight: 600; margin: 1rem 0 0.5rem; }
        .markdown-preview h4 { font-size: 1rem; font-weight: 600; margin: 0.75rem 0 0.4rem; }
        .markdown-preview p { margin: 0.5rem 0; line-height: 1.7; color: hsl(var(--foreground)); opacity: 0.85; }
        .markdown-preview ul, .markdown-preview ol { margin: 0.5rem 0; padding-left: 1.5rem; }
        .markdown-preview li { margin: 0.2rem 0; line-height: 1.6; }
        .markdown-preview li > p { margin: 0; }
        .markdown-preview blockquote { border-left: 3px solid #3b82f6; padding: 0.25rem 1rem; margin: 0.75rem 0; opacity: 0.8; background: rgba(59, 130, 246, 0.05); border-radius: 0 4px 4px 0; }
        .markdown-preview table { border-collapse: collapse; width: 100%; margin: 0.75rem 0; font-size: 0.875rem; }
        .markdown-preview th, .markdown-preview td { border: 1px solid hsl(var(--border)); padding: 0.4rem 0.75rem; text-align: left; }
        .markdown-preview th { font-weight: 600; background: rgba(128,128,128,0.08); }
        .markdown-preview pre { margin: 0.75rem 0; border-radius: 6px; overflow: hidden; }
        .markdown-preview hr { border: none; border-top: 1px solid hsl(var(--border)); margin: 1.25rem 0; }
        .markdown-preview a { color: #3b82f6; text-decoration: none; }
        .markdown-preview a:hover { text-decoration: underline; }
        .markdown-preview img { max-width: 100%; border-radius: 6px; margin: 0.75rem 0; }
        .markdown-preview .hl-inline-code { background: rgba(128,128,128,0.12); padding: 0.15rem 0.4rem; border-radius: 3px; font-size: 0.875em; font-family: 'Cascadia Code','Fira Code','Consolas',monospace; }
        .markdown-preview input[type="checkbox"] { margin-right: 0.4rem; }
      `}</style>
    </div>
  )
}
