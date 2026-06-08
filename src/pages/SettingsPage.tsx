import { useState, useCallback, useEffect, useRef } from 'react'
import {
  ArrowLeft,
  Sun,
  Moon,
  Monitor,
  Palette,
  Globe,
  ChevronRight,
  Shield,
  Brain,
  Save,
  Loader2,
  FileCheck,
} from 'lucide-react'
import { useTheme } from '@/contexts/ThemeContext'
import { useTranslation } from 'react-i18next'
import i18n from '../i18n'
import { getApiBase } from '@/hooks/useApi'

interface SettingsSection {
  id: string
  titleKey: string
  icon: React.ElementType
  iconColor: string
}

const sections: SettingsSection[] = [
  { id: 'appearance', titleKey: 'settings.appearance', icon: Palette, iconColor: 'text-violet-400' },
  { id: 'security', titleKey: 'settings.security', icon: Shield, iconColor: 'text-red-400' },
  { id: 'audit', titleKey: 'settings.audit', icon: FileCheck, iconColor: 'text-cyan-400' },
  { id: 'language', titleKey: 'settings.language', icon: Globe, iconColor: 'text-emerald-400' },
  { id: 'memory', titleKey: 'settings.memory', icon: Brain, iconColor: 'text-amber-400' },
]

interface AppConfig {
  max_iterations: number
  compact_threshold: number
  compact_keep: number
  temperature?: number
  deny_patterns?: string[]
  [key: string]: unknown
}

interface SettingsSettingsProps {
  onBack?: () => void
}

export function SettingsPage({ onBack }: SettingsSettingsProps) {
  const { t, i18n: i18nInstance } = useTranslation()
  const { theme, setTheme, isDark } = useTheme()
  const [activeSection, setActiveSection] = useState<string | null>(null)

  const securityTextareaRef = useRef<HTMLTextAreaElement>(null)
  const memoryTextareaRef = useRef<HTMLTextAreaElement>(null)
  // 记忆管理
  const [memoryContent, setMemoryContent] = useState('')
  const [memoryPath, setMemoryPath] = useState('')
  const [memoryLoading, setMemoryLoading] = useState(false)
  const [memorySaving, setMemorySaving] = useState(false)
  const [memoryStatus, setMemoryStatus] = useState<'idle' | 'saved' | 'error'>('idle')
  const [memoriesDir, setMemoriesDir] = useState('')

  // Agent 配置
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [agentSaveStatus, setAgentSaveStatus] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle')

  // 加载配置
  useEffect(() => {
    fetch(`${getApiBase()}/config`).then(r => r.json()).then(body => {
      if (body.success && body.data) {
        setConfig(body.data)
        if (body.data.memories_dir) {
          setMemoriesDir(body.data.memories_dir)
        }
      }
    }).catch(() => {})
  }, [])

  // 进入记忆页面时加载 MEMORY.md
  useEffect(() => {
    if (activeSection === 'memory' && memoriesDir) {
      const memPath = `${memoriesDir}/MEMORY.md`
      setMemoryPath(memPath)
      setMemoryLoading(true)
      fetch(`${getApiBase()}/files/read`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: memPath }),
      }).then(r => r.json()).then(body => {
        if (body.success) {
          setMemoryContent(body.data || '')
        } else {
          setMemoryContent('')
        }
      }).catch(() => {
        setMemoryContent('')
      }).finally(() => setMemoryLoading(false))
    }
  }, [activeSection, memoriesDir])

  // 进入安全页面时自动撑开黑名单输入框
  useEffect(() => {
    if (activeSection === 'security') {
      requestAnimationFrame(() => {
        const el = securityTextareaRef.current
        if (el) {
          el.style.height = 'auto'
          el.style.height = el.scrollHeight + 'px'
        }
      })
    }
  }, [activeSection, config?.deny_patterns])

  const saveConfig = useCallback(async (updates: Partial<AppConfig>) => {
    if (!config) return
    setAgentSaveStatus('saving')
    try {
      const merged = { ...config, ...updates }
      const res = await fetch(`${getApiBase()}/config`, {
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
    localStorage.setItem('jeeves-language', newLang)
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

  const saveMemory = useCallback(async () => {
    if (!memoryPath) return
    setMemorySaving(true)
    try {
      const res = await fetch(`${getApiBase()}/files/write`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: memoryPath, content: memoryContent }),
      })
      const body = await res.json()
      if (body.success) {
        setMemoryStatus('saved')
        setTimeout(() => setMemoryStatus('idle'), 2500)
      } else {
        setMemoryStatus('error')
        setTimeout(() => setMemoryStatus('idle'), 3000)
      }
    } catch {
      setMemoryStatus('error')
      setTimeout(() => setMemoryStatus('idle'), 3000)
    }
    setMemorySaving(false)
  }, [memoryPath, memoryContent])

  const renderMemory = () => (
    <div className="space-y-4">
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <label className="text-sm font-medium text-foreground/90 mb-1 block">持久记忆</label>
        <p className="text-xs text-foreground/40 mb-3">
          编辑 <code className="text-amber-400">MEMORY.md</code> 文件，保存跨会话的持久记忆。Agent 会在对话中参考这些信息。
        </p>
        {memoryLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-5 h-5 text-foreground/30 animate-spin" />
          </div>
        ) : (
          <textarea
            ref={memoryTextareaRef}
            spellCheck={false}
            value={memoryContent}
            onChange={e => setMemoryContent(e.target.value)}
            placeholder="# 我的记忆&#10;&#10;- 用户偏好: ...&#10;- 项目约定: ...&#10;- 技术选型: ..."
            className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm font-mono text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors resize-y min-h-[300px]"
          />
        )}
      </div>
      <div className="flex items-center gap-3">
        <button
          onClick={saveMemory}
          disabled={memorySaving || memoryLoading}
          className="flex items-center gap-2 px-6 py-2 rounded-lg bg-amber-500 hover:bg-amber-400 disabled:opacity-50 text-sm text-white font-medium transition-colors"
        >
          {memorySaving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
          {memorySaving ? '保存中...' : memoryStatus === 'saved' ? '已保存' : '保存记忆'}
        </button>
        {memoryStatus === 'error' && (
          <span className="text-xs text-red-400/80">保存失败</span>
        )}
      </div>
    </div>
  )

  const renderSecurity = () => (
    <div className="space-y-4">
      {/* 危险命令黑名单 */}
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <label className="text-sm font-medium text-foreground/90 mb-1 block">{t('settings.denyPatterns')}</label>
        <p className="text-xs text-foreground/40 mb-3">{t('settings.denyPatternsDesc')}</p>
        <textarea
          ref={securityTextareaRef}
          spellCheck={false}
          value={(config?.deny_patterns as string[] | undefined)?.join('\n') ?? ''}
          onChange={e => {
            const lines = e.target.value.split('\n').filter(l => l.trim())
            setConfig(prev => prev ? { ...prev, deny_patterns: lines } : null)
          }}
          onInput={() => {
            const el = securityTextareaRef.current
            if (el) { el.style.height = 'auto'; el.style.height = el.scrollHeight + 'px' }
          }}
          placeholder="rm -rf&#10;shutdown&#10;sudo&#10;docker run&#10;pip install&#10;一行一个命令关键词"
          className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs font-mono text-foreground/80 outline-none focus:border-foreground/20 transition-colors resize-none overflow-hidden"
        />
      </div>

      {/* 保存按钮 */}
      <div className="flex items-center gap-3">
        <button
          onClick={() => saveConfig({
            deny_patterns: (config?.deny_patterns as string[] | undefined) ?? [],
          })}
          disabled={agentSaveStatus === 'saving'}
          className="px-6 py-2 rounded-lg bg-red-500 hover:bg-red-400 disabled:opacity-50 text-sm text-white font-medium transition-colors"
        >
          {agentSaveStatus === 'saving' ? t('settings.saving') : agentSaveStatus === 'saved' ? t('settings.saved') : t('settings.save')}
        </button>
        {agentSaveStatus === 'error' && (
          <span className="text-xs text-red-400/80">{t('settings.saveError')}</span>
        )}
      </div>
    </div>
  )

  const renderAudit = () => (
    <div className="space-y-4">
      <div className="rounded-xl border border-border bg-foreground/[0.02] p-4">
        <label className="text-sm font-medium text-foreground/90 mb-3 block">{t('settings.audit')}</label>
        <p className="text-xs text-foreground/50 mb-4">{t('settings.auditDesc')}</p>
        <div className="space-y-3">
          <label className={`flex items-center gap-3 p-3 rounded-lg border cursor-pointer transition-colors ${(config as any)?.approval_mode !== 'auto' ? 'border-blue-500/50 bg-blue-500/10' : 'border-border bg-foreground/5 hover:bg-foreground/10'}`}>
            <input type="radio" name="approval_mode" value="approval"
              checked={(config as any)?.approval_mode !== 'auto'}
              onChange={() => saveConfig({ approval_mode: 'approval' } as any)}
              className="accent-blue-500"
            />
            <div>
              <span className="text-sm font-medium text-foreground/90">{t('settings.auditApproval')}</span>
              <p className="text-xs text-foreground/50 mt-0.5">{t('settings.auditApprovalDesc')}</p>
            </div>
          </label>
          <label className={`flex items-center gap-3 p-3 rounded-lg border cursor-pointer transition-colors ${(config as any)?.approval_mode === 'auto' ? 'border-blue-500/50 bg-blue-500/10' : 'border-border bg-foreground/5 hover:bg-foreground/10'}`}>
            <input type="radio" name="approval_mode" value="auto"
              checked={(config as any)?.approval_mode === 'auto'}
              onChange={() => saveConfig({ approval_mode: 'auto' } as any)}
              className="accent-blue-500"
            />
            <div>
              <span className="text-sm font-medium text-foreground/90">{t('settings.auditAuto')}</span>
              <p className="text-xs text-foreground/50 mt-0.5">{t('settings.auditAutoDesc')}</p>
            </div>
          </label>
        </div>
        {agentSaveStatus === 'saved' && (
          <span className="text-xs text-green-400/80 mt-3 block">{t('settings.saved')}</span>
        )}
        {agentSaveStatus === 'error' && (
          <span className="text-xs text-red-400/80 mt-3 block">{t('settings.saveError')}</span>
        )}
      </div>
    </div>
  )

  const renderSection = () => {
    switch (activeSection) {
      case 'appearance': return renderAppearance()
      case 'security': return renderSecurity()
      case 'audit': return renderAudit()
      case 'language': return renderLanguage()
      case 'memory': return renderMemory()
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


