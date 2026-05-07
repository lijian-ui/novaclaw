import { useState, useMemo, useRef, useCallback } from 'react'
import { ArrowLeft, Save, Copy, Check } from 'lucide-react'

interface FileEditorProps {
  filePath: string
  content: string
  language: string
  onBack: () => void
}

// Simple syntax highlighting for common languages
function highlightCode(code: string, lang: string): string {
  const escaped = code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')

  let highlighted = escaped

  if (['js', 'ts', 'jsx', 'tsx', 'javascript', 'typescript'].includes(lang)) {
    // Strings
    highlighted = highlighted.replace(/(["'`])(?:(?!\1|\\).|\\.)*\1/g, '<span class="hl-string">$&</span>')
    // Numbers
    highlighted = highlighted.replace(/\b(\d+\.?\d*)\b/g, '<span class="hl-number">$1</span>')
    // Keywords
    const keywords = /\b(const|let|var|function|return|if|else|for|while|import|export|from|async|await|class|new|this|throw|try|catch|finally|typeof|instanceof|in|of|true|false|null|undefined)\b/g
    highlighted = highlighted.replace(keywords, '<span class="hl-keyword">$1</span>')
    // Comments
    highlighted = highlighted.replace(/(\/\/.*)/g, '<span class="hl-comment">$1</span>')
    highlighted = highlighted.replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="hl-comment">$1</span>')
  } else if (['html', 'htm'].includes(lang)) {
    // Tags
    highlighted = highlighted.replace(/(&lt;\/?[\w-]+)/g, '<span class="hl-tag">$1</span>')
    highlighted = highlighted.replace(/(\/?&gt;)/g, '<span class="hl-tag">$1</span>')
    // Attributes
    highlighted = highlighted.replace(/(\s[\w-]+)(=)/g, '<span class="hl-attr">$1</span>$2')
    // Strings
    highlighted = highlighted.replace(/(["'])(?:(?!\1|\\).|\\.)*\1/g, '<span class="hl-string">$&</span>')
    // Comments
    highlighted = highlighted.replace(/(&lt;!--[\s\S]*?--&gt;)/g, '<span class="hl-comment">$1</span>')
  } else if (['css', 'scss', 'less'].includes(lang)) {
    // Selectors
    highlighted = highlighted.replace(/([\w.#@][\w.#@\s,>:]+)\s*\{/g, '<span class="hl-selector">$1</span> {')
    // Properties
    highlighted = highlighted.replace(/([\w-]+)(\s*:\s*)/g, '<span class="hl-property">$1</span>$2')
    // Values
    highlighted = highlighted.replace(/(#[\da-fA-F]{3,8})\b/g, '<span class="hl-number">$1</span>')
    highlighted = highlighted.replace(/(\d+\.?\d*(px|rem|em|vh|vw|%|s|ms)?)\b/g, '<span class="hl-number">$1</span>')
    // Strings
    highlighted = highlighted.replace(/(["'])(?:(?!\1|\\).|\\.)*\1/g, '<span class="hl-string">$&</span>')
    // Comments
    highlighted = highlighted.replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="hl-comment">$1</span>')
  } else if (['json'].includes(lang)) {
    // Keys
    highlighted = highlighted.replace(/"([^"]+)"\s*:/g, '<span class="hl-attr">"$1"</span>:')
    // Strings
    highlighted = highlighted.replace(/(:\s*)"([^"]*)"(,?)/g, '$1<span class="hl-string">"$2"</span>$3')
    // Numbers
    highlighted = highlighted.replace(/(:\s*)(\d+\.?\d*)/g, '$1<span class="hl-number">$2</span>')
    // Booleans/null
    highlighted = highlighted.replace(/\b(true|false|null)\b/g, '<span class="hl-keyword">$1</span>')
  }

  // Line numbers
  const lines = highlighted.split('\n')
  return lines
    .map((line, i) => {
      const lineNum = String(i + 1).padStart(4, ' ')
      return `<span class="hl-line-num">${lineNum}</span> ${line || ' '}`
    })
    .join('\n')
}

const fileLangMap: Record<string, string> = {
  ts: 'typescript', tsx: 'tsx', js: 'javascript', jsx: 'jsx',
  html: 'html', css: 'css', json: 'json', md: 'markdown',
}

function getLanguage(filePath: string): string {
  const ext = filePath.split('.').pop() || ''
  return fileLangMap[ext] || ext
}

export function FileEditor({ filePath, content: initialContent, onBack }: FileEditorProps) {
  const [content, setContent] = useState(initialContent)
  const [copied, setCopied] = useState(false)
  const [saved, setSaved] = useState(false)
  const language = useMemo(() => getLanguage(filePath), [filePath])
  const highlighted = useMemo(() => highlightCode(content, language), [content, language])

  const fileName = filePath.split('/').pop() || filePath
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const preRef = useRef<HTMLPreElement>(null)

  const syncScroll = useCallback(() => {
    if (textareaRef.current && preRef.current) {
      preRef.current.scrollTop = textareaRef.current.scrollTop
      preRef.current.scrollLeft = textareaRef.current.scrollLeft
    }
  }, [])

  const handleCopy = () => {
    navigator.clipboard.writeText(content).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

  const handleSave = () => {
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3 min-w-0">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors shrink-0">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <div className="min-w-0">
            <span className="text-sm font-medium text-foreground/90 truncate block">{fileName}</span>
          </div>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            onClick={handleCopy}
            className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
            title="复制代码"
          >
            {copied ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4 text-foreground/50" />}
          </button>
          <button
            onClick={handleSave}
            className="flex items-center gap-1 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 text-xs text-white font-medium transition-colors"
          >
            <Save className="w-3.5 h-3.5" />
            {saved ? '已保存' : '保存'}
          </button>
        </div>
      </div>

      {/* Editor */}
      <div className="flex-1 overflow-hidden flex">
        <div className="flex-1 relative">
          {/* Highlighted overlay */}
          <pre
            ref={preRef}
            className="absolute inset-0 m-0 p-4 font-mono text-sm leading-relaxed pointer-events-none overflow-hidden"
            dangerouslySetInnerHTML={{ __html: highlighted }}
            aria-hidden="true"
          />
          {/* Editable textarea */}
          <textarea
            ref={textareaRef}
            value={content}
            onChange={e => setContent(e.target.value)}
            onScroll={syncScroll}
            className="absolute inset-0 w-full h-full p-4 font-mono text-sm leading-relaxed bg-transparent text-transparent caret-foreground outline-none resize-none whitespace-pre overflow-auto"
            spellCheck={false}
          />
        </div>
      </div>

      {/* Bottom status bar */}
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
        .hl-line-num { color: #5c6370; user-select: none; display: inline-block; width: 3em; text-align: right; padding-right: 1.5em; }

        .light .hl-keyword { color: #8250df; }
        .light .hl-string { color: #0a3069; }
        .light .hl-number { color: #0550ae; }
        .light .hl-comment { color: #6e7781; font-style: italic; }
        .light .hl-tag { color: #cf222e; }
        .light .hl-attr { color: #0550ae; }
        .light .hl-selector { color: #8250df; }
        .light .hl-property { color: #0550ae; }
        .light .hl-line-num { color: #6e7781; }
      `}</style>
    </div>
  )
}
