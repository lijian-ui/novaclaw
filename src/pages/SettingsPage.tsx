import { useState } from 'react'
import {
  ArrowLeft,
  Sun,
  Moon,
  Monitor,
  MessageSquare,
  Bot,
  Palette,
  Type,
  Globe,
  ChevronRight,
} from 'lucide-react'
import { useTheme } from '@/contexts/ThemeContext'

interface SettingsSection {
  id: string
  title: string
  icon: React.ElementType
  iconColor: string
}

const sections: SettingsSection[] = [
  { id: 'appearance', title: '外观', icon: Palette, iconColor: 'text-violet-400' },
  { id: 'chat', title: '对话', icon: MessageSquare, iconColor: 'text-blue-400' },
  { id: 'language', title: '语言', icon: Globe, iconColor: 'text-emerald-400' },
]

interface SettingsSettingsProps {
  onBack?: () => void
}

export function SettingsPage({ onBack }: SettingsSettingsProps) {
  const { theme, toggle } = useTheme()
  const [activeSection, setActiveSection] = useState<string | null>(null)

  const renderAppearance = () => (
    <div className="space-y-4">
      {/* 主题 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <h4 className="text-sm font-medium text-foreground/90 mb-3">主题</h4>
        <div className="grid grid-cols-3 gap-3">
          <button
            onClick={() => { if (theme !== 'light') toggle() }}
            className={`flex flex-col items-center gap-2 px-4 py-3 rounded-lg border text-xs transition-colors ${
              theme === 'light'
                ? 'border-blue-500/50 bg-blue-500/10 text-blue-400'
                : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
            }`}
          >
            <Sun className="w-5 h-5" />
            明亮
          </button>
          <button
            onClick={() => { if (theme !== 'dark') toggle() }}
            className={`flex flex-col items-center gap-2 px-4 py-3 rounded-lg border text-xs transition-colors ${
              theme === 'dark'
                ? 'border-blue-500/50 bg-blue-500/10 text-blue-400'
                : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
            }`}
          >
            <Moon className="w-5 h-5" />
            暗色
          </button>
          <button
            className={`flex flex-col items-center gap-2 px-4 py-3 rounded-lg border text-xs transition-colors ${
              false
                ? 'border-blue-500/50 bg-blue-500/10 text-blue-400'
                : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
            }`}
          >
            <Monitor className="w-5 h-5" />
            跟随系统
          </button>
        </div>
      </div>

      {/* 字体大小 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <h4 className="text-sm font-medium text-foreground/90 mb-3">字体大小</h4>
        <div className="flex items-center gap-3">
          <Type className="w-4 h-4 text-foreground/40 shrink-0" />
          <input
            type="range"
            min="12"
            max="20"
            defaultValue="14"
            className="flex-1 accent-blue-500 h-1.5 rounded-full appearance-none cursor-pointer bg-foreground/10 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-blue-500"
          />
          <span className="text-xs text-foreground/50 w-8 text-right">14px</span>
        </div>
      </div>
    </div>
  )

  const renderChat = () => (
    <div className="space-y-4">
      {/* 默认助手 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-blue-500/10">
              <Bot className="w-4 h-4 text-blue-400" />
            </div>
            <div>
              <p className="text-sm font-medium text-foreground/90">默认助手</p>
              <p className="text-xs text-foreground/40 mt-0.5">选择对话使用的默认模型</p>
            </div>
          </div>
          <select className="px-3 py-1.5 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none cursor-pointer">
            <option>自动选择</option>
          </select>
        </div>
      </div>

      {/* 消息发送 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-foreground/5">
              <MessageSquare className="w-4 h-4 text-foreground/50" />
            </div>
            <div>
              <p className="text-sm font-medium text-foreground/90">Enter 发送消息</p>
              <p className="text-xs text-foreground/40 mt-0.5">按 Enter 发送，Shift+Enter 换行</p>
            </div>
          </div>
          <button
            className={`relative w-8 h-4 rounded-full transition-colors ${
              true ? 'bg-green-500' : 'bg-foreground/20'
            }`}
          >
            <div
              className={`absolute top-0.5 w-3 h-3 rounded-full bg-foreground transition-transform ${
                true ? 'translate-x-4' : 'translate-x-0.5'
              }`}
            />
          </button>
        </div>
      </div>

      {/* 代码高亮 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-foreground/5">
              <Type className="w-4 h-4 text-foreground/50" />
            </div>
            <div>
              <p className="text-sm font-medium text-foreground/90">代码高亮</p>
              <p className="text-xs text-foreground/40 mt-0.5">在消息中启用代码语法高亮</p>
            </div>
          </div>
          <button
            className={`relative w-8 h-4 rounded-full transition-colors ${
              true ? 'bg-green-500' : 'bg-foreground/20'
            }`}
          >
            <div
              className={`absolute top-0.5 w-3 h-3 rounded-full bg-foreground transition-transform ${
                true ? 'translate-x-4' : 'translate-x-0.5'
              }`}
            />
          </button>
        </div>
      </div>
    </div>
  )

  const renderLanguage = () => (
    <div className="space-y-4">
      {/* 界面语言 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-emerald-500/10">
              <Globe className="w-4 h-4 text-emerald-400" />
            </div>
            <div>
              <p className="text-sm font-medium text-foreground/90">界面语言</p>
              <p className="text-xs text-foreground/40 mt-0.5">选择应用程序的显示语言</p>
            </div>
          </div>
          <select className="px-3 py-1.5 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none cursor-pointer">
            <option>简体中文</option>
            <option>English</option>
            <option>日本語</option>
          </select>
        </div>
      </div>
    </div>
  )

  const renderSection = () => {
    switch (activeSection) {
      case 'appearance': return renderAppearance()
      case 'chat': return renderChat()
      case 'language': return renderLanguage()
      default: return null
    }
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">
            {activeSection
              ? sections.find(s => s.id === activeSection)?.title
              : '设置'
            }
          </span>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-4 py-4">
        {activeSection ? (
          <div>
            <button
              onClick={() => setActiveSection(null)}
              className="flex items-center gap-1 text-xs text-foreground/50 hover:text-foreground/80 transition-colors mb-4"
            >
              <ChevronRight className="w-3 h-3 rotate-180" />
              返回设置
            </button>
            {renderSection()}
          </div>
        ) : (
          <div className="space-y-2">
            {sections.map((section) => {
              const Icon = section.icon
              return (
                <button
                  key={section.id}
                  onClick={() => setActiveSection(section.id)}
                  className="w-full flex items-center justify-between px-4 py-3 rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors"
                >
                  <div className="flex items-center gap-3">
                    <div className="p-2 rounded-lg bg-foreground/5">
                      <Icon className={`w-4 h-4 ${section.iconColor}`} />
                    </div>
                    <span className="text-sm font-medium text-foreground/90">{section.title}</span>
                  </div>
                  <ChevronRight className="w-4 h-4 text-foreground/30" />
                </button>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
