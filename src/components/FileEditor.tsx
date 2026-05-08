import { useState, useMemo, useRef, useCallback, useEffect } from 'react'
import { X, Save, Copy, Check } from 'lucide-react'
import type { EditorTab } from '@/types/fileEditor'

interface FileEditorProps {
  tabs: EditorTab[]
  activeTab: EditorTab | null
  onUpdateContent: (content: string) => void
  onSave: () => void
  onCloseTab: (path: string) => void
  onSwitchTab: (path: string) => void
}

// ---- 语法高亮 ----

function highlightCode(code: string, lang: string): string {
  const escaped = code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')

  let highlighted = escaped

  if (['js', 'ts', 'jsx', 'tsx', 'javascript', 'typescript'].includes(lang)) {
    highlighted = highlighted.replace(/(["'`])(?:(?!\1|\\).|\\.)*\1/g, '<span class="hl-string">$&</span>')
    highlighted = highlighted.replace(/\b(\d+\.?\d*)\b/g, '<span class="hl-number">$1</span>')
    const keywords = /\b(const|let|var|function|return|if|else|for|while|import|export|from|async|await|class|new|this|throw|try|catch|finally|typeof|instanceof|in|of|true|false|null|undefined)\b/g
    highlighted = highlighted.replace(keywords, '<span class="hl-keyword">$1</span>')
    highlighted = highlighted.replace(/(\/\/.*)/g, '<span class="hl-comment">$1</span>')
    highlighted = highlighted.replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="hl-comment">$1</span>')
  } else if (['html', 'htm'].includes(lang)) {
    highlighted = highlighted.replace(/(&lt;\/?[\w-]+)/g, '<span class="hl-tag">$1</span>')
    highlighted = highlighted.replace(/(\/?&gt;)/g, '<span class="hl-tag">$1</span>')
    highlighted = highlighted.replace(/(\s[\w-]+)(=)/g, '<span class="hl-attr">$1</span>$2')
    highlighted = highlighted.replace(/(["'])(?:(?!\1|\\).|\\.)*\1/g, '<span class="hl-string">$&</span>')
    highlighted = highlighted.replace(/(&lt;!--[\s\S]*?--&gt;)/g, '<span class="hl-comment">$1</span>')
  } else if (['css', 'scss', 'less'].includes(lang)) {
    highlighted = highlighted.replace(/([\w.#@][\w.#@\s,>:]+)\s*\{/g, '<span class="hl-selector">$1</span> {')
    highlighted = highlighted.replace(/([\w-]+)(\s*:\s*)/g, '<span class="hl-property">$1</span>$2')
    highlighted = highlighted.replace(/(#[\da-fA-F]{3,8})\b/g, '<span class="hl-number">$1</span>')
    highlighted = highlighted.replace(/(\d+\.?\d*(px|rem|em|vh|vw|%|s|ms)?)\b/g, '<span class="hl-number">$1</span>')
    highlighted = highlighted.replace(/(["'])(?:(?!\1|\\).|\\.)*\1/g, '<span class="hl-string">$&</span>')
    highlighted = highlighted.replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="hl-comment">$1</span>')
  } else if (['json'].includes(lang)) {
    highlighted = highlighted.replace(/"([^"]+)"\s*:/g, '<span class="hl-attr">"$1"</span>:')
    highlighted = highlighted.replace(/(:\s*)"([^"]*)"(,?)/g, '$1<span class="hl-string">"$2"</span>$3')
    highlighted = highlighted.replace(/(:\s*)(\d+\.?\d*)/g, '$1<span class="hl-number">$2</span>')
    highlighted = highlighted.replace(/\b(true|false|null)\b/g, '<span class="hl-keyword">$1</span>')
  }

  // 不再嵌入行号，返回纯高亮 HTML
  return highlighted
}

/** Markdown 实时预览 */
function MarkdownPreview({ content }: { content: string }) {
  const html = useMemo(() => {
    let h = content
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
    // 标题
    h = h.replace(/^### (.+)$/gm, '<h3>$1</h3>')
    h = h.replace(/^## (.+)$/gm, '<h2>$1</h2>')
    h = h.replace(/^# (.+)$/gm, '<h1>$1</h1>')
    // 代码块
    h = h.replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="hl-code-block"><code>$2</code></pre>')
    // 行内代码
    h = h.replace(/`([^`]+)`/g, '<code class="hl-inline-code">$1</code>')
    // 加粗
    h = h.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    // 列表
    h = h.replace(/^- (.+)$/gm, '<li>$1</li>')
    // 链接
    h = h.replace(/\[(.+?)\]\((.+?)\)/g, '<a href="$2" target="_blank">$1</a>')
    // 段落
    h = h.replace(/^(?!<[hlp]).+$/gm, '<p>$&</p>')
    return h
  }, [content])

  return (
    <div
      className="p-4 text-sm text-foreground/80 leading-relaxed overflow-y-auto h-full prose prose-sm dark:prose-invert"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}

export function FileEditor({ tabs, activeTab, onUpdateContent, onSave, onCloseTab, onSwitchTab }: FileEditorProps) {
  const [copied, setCopied] = useState(false)

  const content = activeTab?.content || ''
  const language = activeTab?.language || ''
  const fileName = activeTab?.name || ''
  const dirty = activeTab?.dirty || false
  const isMarkdown = language === 'markdown'
  const [preview, setPreview] = useState(isMarkdown)

  const highlighted = useMemo(() => highlightCode(content, language), [content, language])
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const preRef = useRef<HTMLPreElement>(null)
  const gutterRef = useRef<HTMLDivElement>(null)

  const lineCount = content.split('\n').length
  const lineNumArr = useMemo(() => Array.from({ length: lineCount }, (_, i) => i + 1), [lineCount])

  const syncScroll = useCallback(() => {
    if (textareaRef.current && preRef.current) {
      preRef.current.scrollTop = textareaRef.current.scrollTop
      preRef.current.scrollLeft = textareaRef.current.scrollLeft
    }
    if (gutterRef.current && textareaRef.current) {
      gutterRef.current.scrollTop = textareaRef.current.scrollTop
    }
  }, [])

  const handleCopy = () => {
    navigator.clipboard.writeText(content).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

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
          <button onClick={handleCopy} className="p-1.5 rounded hover:bg-foreground/10 transition-colors" title="复制">
            {copied ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4 text-foreground/50" />}
          </button>
          {isMarkdown && (
            <button
              onClick={() => setPreview(p => !p)}
              className={`px-2 py-1 text-xs rounded transition-colors ${preview ? 'bg-blue-500/20 text-blue-400' : 'text-foreground/50 hover:text-foreground/70 hover:bg-foreground/10'}`}
            >
              预览
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

      {/* Editor + Preview */}
      <div className="flex-1 overflow-hidden flex">
        {/* Editor pane */}
        <div className={`flex flex-row ${preview ? 'w-1/2 border-r border-border' : 'w-full'}`}>
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
            <pre
              ref={preRef}
              className="absolute inset-0 m-0 p-4 font-mono text-sm leading-relaxed pointer-events-none overflow-hidden"
              dangerouslySetInnerHTML={{ __html: highlighted }}
              aria-hidden="true"
            />
            <textarea
              ref={textareaRef}
              value={content}
              onChange={handleContentChange}
              onScroll={syncScroll}
              className="absolute inset-0 w-full h-full p-4 font-mono text-sm leading-relaxed bg-transparent text-transparent caret-foreground outline-none resize-none whitespace-pre overflow-auto"
              spellCheck={false}
            />
          </div>
        </div>

        {/* Preview pane */}
        {preview && isMarkdown && (
          <div className="flex-1 w-1/2 bg-background">
            <MarkdownPreview content={content} />
          </div>
        )}
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between px-4 py-1.5 border-t border-border bg-foreground/[0.02] shrink-0">
        <span className="text-[10px] text-foreground/40 font-mono">{language.toUpperCase()}</span>
        <span className="text-[10px] text-foreground/40 font-mono">
          {content.split('\n').length} 行 | {content.length} 字符
        </span>
      </div>

      {/* Highlight styles */}
      <style>{`
        .hl-keyword { color: #c678dd; }
        .hl-string { color: #98c379; }
        .hl-number { color: #d19a66; }
        .hl-comment { color: #5c6370; font-style: italic; }
        .hl-tag { color: #e06c75; }
        .hl-attr { color: #d19a66; }
        .hl-selector { color: #e06c75; }
        .hl-property { color: #61afef; }
        .hl-code-block { background: #1e2329; border-radius: 8px; padding: 12px; overflow-x: auto; margin: 8px 0; }
        .hl-inline-code { background: #1e2329; padding: 1px 4px; border-radius: 3px; font-size: 0.9em; }
        .light .hl-keyword { color: #8250df; }
        .light .hl-string { color: #0a3069; }
        .light .hl-number { color: #0550ae; }
        .light .hl-comment { color: #6e7781; font-style: italic; }
        .light .hl-tag { color: #cf222e; }
        .light .hl-attr { color: #0550ae; }
        .light .hl-selector { color: #8250df; }
        .light .hl-property { color: #0550ae; }
        .light .hl-code-block { background: #f3f3f3; }
        .light .hl-inline-code { background: #f3f3f3; }
      `}</style>
    </div>
  )
}
