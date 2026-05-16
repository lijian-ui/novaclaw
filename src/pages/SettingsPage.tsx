import { useState, useCallback, useEffect } from 'react'
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
  Cpu,
} from 'lucide-react'
import { useTheme } from '@/contexts/ThemeContext'
import { useTranslation } from 'react-i18next'
import i18n from '../i18n'

interface SettingsSection {
  id: string
  titleKey: string
  icon: React.ElementType
  iconColor: string
}

const sections: SettingsSection[] = [
  { id: 'appearance', titleKey: 'settings.appearance', icon: Palette, iconColor: 'text-violet-400' },
  { id: 'chat', titleKey: 'settings.chat', icon: MessageSquare, iconColor: 'text-blue-400' },
  { id: 'agent', titleKey: 'settings.agent', icon: Cpu, iconColor: 'text-orange-400' },
  { id: 'language', titleKey: 'settings.language', icon: Globe, iconColor: 'text-emerald-400' },
]

const CONFIG_API = 'http://127.0.0.1:3000/api/config'

interface AppConfig {
  max_iterations: number
  compact_threshold: number
  compact_keep: number
  [key: string]: unknown
}

interface SettingsSettingsProps {
  onBack?: () => void
}

export function SettingsPage({ onBack }: SettingsSettingsProps) {
  const { t, i18n: i18nInstance } = useTranslation()
  const { theme, setTheme, isDark } = useTheme()
  const [activeSection, setActiveSection] = useState<string | null>(null)

  // Agent 配置
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [agentSaveStatus, setAgentSaveStatus] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle')

  // 加载配置
  useEffect(() => {
    fetch(CONFIG_API).then(r => r.json()).then(body => {
      if (body.success && body.data) {
        setConfig(body.data)
      }
    }).catch(() => {})
  }, [])

  const saveConfig = useCallback(async (updates: Partial<AppConfig>) => {
    if (!config) return
    setAgentSaveStatus('saving')
    try {
      const merged = { ...config, ...updates }
      const res = await fetch(CONFIG_API, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(merged),
      })
      const body = await res.json()
      if (body.success) {
        setConfig(merged)
        setAgentSaveStatus('saved')
        setTimeout(() => setAgentSaveStatus('idle'), 2500)
      } else {
        setAgentSaveStatus('error')
        setTimeout(() => setAgentSaveStatus('idle'), 3000)
      }
    } catch {
      setAgentSaveStatus('error')
      setTimeout(() => setAgentSaveStatus('idle'), 3000)
    }
  }, [config])

  const handleLanguageChange = useCallback((newLang: string) => {
    i18n.changeLanguage(newLang)
    localStorage.setItem('novaclaw-language', newLang)
  }, [])

  const renderAppearance = () => (
    <div className="space-y-4">
      {/* 主题 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <h4 className="text-sm font-medium text-foreground/90 mb-3">{t('settings.theme')}</h4>
        <div className="grid grid-cols-3 gap-3">
          <button
            onClick={() => setTheme('light')}
            className={`flex flex-col items-center gap-2 px-4 py-3 rounded-lg border text-xs transition-colors ${
              theme === 'light'
                ? 'border-blue-500/50 bg-blue-500/10 text-blue-400'
                : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
            }`}
          >
            <Sun className="w-5 h-5" />
            {t('settings.light')}
          </button>
          <button
            onClick={() => setTheme('dark')}
            className={`flex flex-col items-center gap-2 px-4 py-3 rounded-lg border text-xs transition-colors ${
              theme === 'dark'
                ? 'border-blue-500/50 bg-blue-500/10 text-blue-400'
                : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
            }`}
          >
            <Moon className="w-5 h-5" />
            {t('settings.dark')}
          </button>
          <button
            onClick={() => setTheme('system')}
            className={`flex flex-col items-center gap-2 px-4 py-3 rounded-lg border text-xs transition-colors ${
              theme === 'system'
                ? 'border-blue-500/50 bg-blue-500/10 text-blue-400'
                : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
            }`}
          >
            <Monitor className="w-5 h-5" />
            {t('settings.system')}
          </button>
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
              <p className="text-sm font-medium text-foreground/90">{t('settings.defaultAssistant')}</p>
              <p className="text-xs text-foreground/40 mt-0.5">{t('settings.selectModel')}</p>
            </div>
          </div>
          <select className="px-3 py-1.5 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none cursor-pointer">
            <option>{t('settings.autoSelect')}</option>
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
              <p className="text-sm font-medium text-foreground/90">{t('settings.enterSend')}</p>
              <p className="text-xs text-foreground/40 mt-0.5">{t('settings.enterSendDesc')}</p>
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
              <p className="text-sm font-medium text-foreground/90">{t('settings.codeHighlight')}</p>
              <p className="text-xs text-foreground/40 mt-0.5">{t('settings.codeHighlightDesc')}</p>
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

  const renderAgent = () => (
    <div className="space-y-4">
      {/* 最大迭代次数 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <label className="text-sm font-medium text-foreground/90 mb-1 block">{t('settings.agentMaxIterations')}</label>
        <p className="text-xs text-foreground/40 mb-3">{t('settings.agentMaxIterationsDesc')}</p>
        <input
          type="number" min={0} max={999}
          value={config?.max_iterations ?? 0}
          onChange={e => setConfig(prev => prev ? { ...prev, max_iterations: parseInt(e.target.value) || 0 } : null)}
          className="w-24 px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 outline-none focus:border-foreground/20 transition-colors"
        />
      </div>

      {/* Temperature */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <label className="text-sm font-medium text-foreground/90 mb-1 block">{t('settings.temperature')}</label>
        <p className="text-xs text-foreground/40 mb-3">{t('settings.temperatureDesc')}</p>
        <div className="flex items-center gap-4">
          <input
            type="range" min={0} max={200} step={5}
            value={Math.round((config?.temperature ?? 0.7) * 100)}
            onChange={e => setConfig(prev => prev ? { ...prev, temperature: parseInt(e.target.value) / 100 } : null)}
            className="flex-1 h-1.5 bg-foreground/10 rounded-lg appearance-none cursor-pointer accent-blue-500"
          />
          <span className="text-sm text-foreground/70 font-mono w-10 text-right">{(config?.temperature ?? 0.7).toFixed(2)}</span>
        </div>
      </div>

      {/* 上下文压缩 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <label className="text-sm font-medium text-foreground/90 mb-1 block">{t('settings.compactThreshold')}</label>
        <p className="text-xs text-foreground/40 mb-3">{t('settings.compactThresholdDesc')}</p>
        <div className="flex items-center gap-3">
          <span className="text-xs text-foreground/50">{t('settings.compactAfter')}</span>
          <input
            type="number" min={0} max={999}
            value={config?.compact_threshold ?? 40}
            onChange={e => setConfig(prev => prev ? { ...prev, compact_threshold: parseInt(e.target.value) || 0 } : null)}
            className="w-20 px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 outline-none focus:border-foreground/20 transition-colors"
          />
          <span className="text-xs text-foreground/50">{t('settings.compactMessages')}</span>
          <span className="text-xs text-foreground/50 ml-1">{t('settings.keep')}</span>
          <input
            type="number" min={1} max={200}
            value={config?.compact_keep ?? 20}
            onChange={e => setConfig(prev => prev ? { ...prev, compact_keep: parseInt(e.target.value) || 20 } : null)}
            className="w-20 px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 outline-none focus:border-foreground/20 transition-colors"
          />
          <span className="text-xs text-foreground/50">{t('settings.compactLatest')}</span>
        </div>
      </div>

      {/* 统一保存按钮 */}
      <div className="flex items-center gap-3">
        <button
          onClick={() => saveConfig({
            max_iterations: config?.max_iterations ?? 0,
            temperature: config?.temperature ?? 0.7,
            compact_threshold: config?.compact_threshold ?? 40,
            compact_keep: config?.compact_keep ?? 20,
          })}
          disabled={agentSaveStatus === 'saving'}
          className="px-6 py-2 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-sm text-white font-medium transition-colors"
        >
          {agentSaveStatus === 'saving' ? t('settings.saving') : agentSaveStatus === 'saved' ? t('settings.saved') : t('settings.saveAgent')}
        </button>
        {agentSaveStatus === 'error' && (
          <span className="text-xs text-red-400/80">{t('settings.saveError')}</span>
        )}
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
              <p className="text-sm font-medium text-foreground/90">{t('settings.uiLanguage')}</p>
              <p className="text-xs text-foreground/40 mt-0.5">{t('settings.selectLanguage')}</p>
            </div>
          </div>
          <select 
            value={i18nInstance.language}
            onChange={(e) => handleLanguageChange(e.target.value)}
            className={`px-3 py-1.5 rounded-lg border text-xs outline-none cursor-pointer appearance-none w-32 text-center relative transition-colors ${
              isDark 
                ? 'bg-gray-800 border-gray-600 text-gray-200 hover:bg-gray-700' 
                : 'bg-gray-100 border-gray-300 text-gray-800 hover:bg-gray-200'
            }`}
            style={{
              backgroundImage: `url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b7280' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e")`,
              backgroundPosition: 'right 0.5rem center',
              backgroundRepeat: 'no-repeat',
              backgroundSize: '1.5em 1.5em',
            }}
          >
            <option value="zh-CN">简体中文</option>
            <option value="en-US">English</option>
          </select>
        </div>
      </div>
    </div>
  )

  const renderSection = () => {
    switch (activeSection) {
      case 'appearance': return renderAppearance()
      case 'chat': return renderChat()
      case 'agent': return renderAgent()
      case 'language': return renderLanguage()
      default: return null
    }
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3.5 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">
            {activeSection
              ? t(sections.find(s => s.id === activeSection)?.titleKey || '')
              : t('settings.title')
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
              {t('settings.back')}
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
                    <span className="text-sm font-medium text-foreground/90">{t(section.titleKey)}</span>
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
