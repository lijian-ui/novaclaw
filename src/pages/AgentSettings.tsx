import { useState, useEffect, useCallback, useRef } from 'react'
import { Plus, Pencil, Trash2, X, Brain, ArrowLeft, ChevronDown, Check, Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { getApiBase } from '@/hooks/useApi'

interface SubAgentProfile {
  id: string
  name: string
  description: string
  system_prompt: string
  model: string | null
  enabled_tools: string[]
  max_iterations: number
  temperature: number | null
  compact_threshold: number | null
  compact_keep: number | null
}

interface AgentSettingsProps {
  onBack?: () => void
}

export function AgentSettings({ onBack }: AgentSettingsProps) {
  const { t } = useTranslation()
  const [profiles, setProfiles] = useState<SubAgentProfile[]>([])
  const [loading, setLoading] = useState(true)
  const [editingProfile, setEditingProfile] = useState<SubAgentProfile | null>(null)
  const [showProfileForm, setShowProfileForm] = useState(false)
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null)

  const loadProfiles = useCallback(() => {
    setLoading(true)
    fetch(`${getApiBase()}/agents`).then(r => r.json()).then(body => {
      if (body.success && Array.isArray(body.data)) {
        const items: SubAgentProfile[] = body.data.map((a: any) => ({
          id: a.id,
          name: a.name,
          description: a.description || '',
          system_prompt: '',
          model: a.model || null,
          enabled_tools: a.enabled_tools || [],
          max_iterations: a.max_iterations ?? 0,
          temperature: a.temperature ?? null,
          compact_threshold: a.compact_threshold ?? null,
          compact_keep: a.compact_keep ?? null,
        }))
        setProfiles(items)
      }
    }).catch(() => {}).finally(() => setLoading(false))
  }, [])
  useEffect(() => { loadProfiles() }, [loadProfiles])

  const handleSaveProfile = useCallback(async (profile: SubAgentProfile) => {
    try {
      const body: any = {
        name: profile.name,
        description: profile.description,
        model: profile.model || '',
        enabled_tools: profile.enabled_tools,
        max_iterations: profile.max_iterations,
        temperature: profile.temperature,
        compact_threshold: profile.compact_threshold,
        compact_keep: profile.compact_keep,
      }
      if (profile.system_prompt) {
        body.system_prompt = profile.system_prompt
      }
      const res = await fetch(`${getApiBase()}/agents/${profile.id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
      const result = await res.json()
      if (result.success) {
        setShowProfileForm(false)
        setEditingProfile(null)
        loadProfiles()
      }
    } catch {}
  }, [loadProfiles])

  const handleDeleteProfile = useCallback(async (id: string) => {
    if (id === 'default') return
    try {
      await fetch(`${getApiBase()}/agents/${id}`, { method: 'DELETE' })
      setDeleteConfirm(null)
      loadProfiles()
    } catch {}
  }, [loadProfiles])

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('agentSettings.title')}</span>
        </div>
        {!showProfileForm && (
          <button onClick={() => { setEditingProfile(null); setShowProfileForm(true) }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 text-white text-xs font-medium transition-colors">
            <Plus className="w-3.5 h-3.5" />{t('agentSettings.addSubAgent')}
          </button>
        )}
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {showProfileForm ? (
          <ProfileForm
            initial={editingProfile}
            onSave={handleSaveProfile}
            onCancel={() => { setShowProfileForm(false); setEditingProfile(null) }}
          />
        ) : loading ? (
          <div className="flex items-center justify-center h-full"><Loader2 className="w-5 h-5 animate-spin text-foreground/30" /></div>
        ) : profiles.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Brain className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">{t('settings.noEmployees')}</p>
          </div>
        ) : (
          <div className="space-y-2">
            {[...profiles].sort((a, b) => a.id === 'default' ? -1 : b.id === 'default' ? 1 : 0).map((profile) => {
              const isDefault = profile.id === 'default'
              return (
                <div key={profile.id} className={`rounded-xl border p-3.5 ${
                  isDefault ? 'border-orange-500/30 bg-orange-500/[0.03]' : 'border-border bg-foreground/[0.02]'
                }`}>
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <Brain className={`w-4 h-4 shrink-0 ${isDefault ? 'text-orange-400' : 'text-cyan-400'}`} />
                        <span className="text-sm font-medium text-foreground/90">{isDefault ? 'Jeeves' : profile.name}</span>
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/50 font-mono">{profile.id}</span>
                        {isDefault && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded bg-orange-500/10 text-orange-400 border border-orange-500/20">默认</span>
                        )}
                      </div>
                      <p className="text-xs text-foreground/50 mt-1 line-clamp-2">{isDefault && !profile.description ? '系统默认智能体' : profile.description}</p>
                      <div className="flex items-center gap-3 mt-2 text-[10px] text-foreground/40 flex-wrap">
                        <span>{profile.model || t('settings.inheritModel')}</span>
                        <span>{profile.enabled_tools.length} 工具</span>
                        <span>{t('settings.maxIter', { n: profile.max_iterations })}</span>
                        {profile.temperature !== null && <span>温度 {profile.temperature.toFixed(2)}</span>}
                        {profile.compact_threshold !== null && <span>压缩 {profile.compact_threshold}→{profile.compact_keep}</span>}
                      </div>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button onClick={async () => {
                          // 加载 SOUL.md 内容
                          try {
                            const res = await fetch(`${getApiBase()}/agents/${profile.id}/soul`)
                            const body = await res.json()
                            if (body.success) {
                              profile.system_prompt = body.data || ''
                            }
                          } catch {}
                          setEditingProfile(profile); setShowProfileForm(true)
                        }}
                        className="p-1.5 rounded hover:bg-foreground/10 transition-colors">
                        <Pencil className="w-3.5 h-3.5 text-foreground/40" />
                      </button>
                      {!isDefault && (
                        <button onClick={() => setDeleteConfirm(profile.id)}
                          className="p-1.5 rounded hover:bg-red-500/10 transition-colors">
                          <Trash2 className="w-3.5 h-3.5 text-red-400/60" />
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>

      {deleteConfirm && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="w-full max-w-sm rounded-xl border border-border bg-card shadow-2xl">
            <div className="px-4 py-4">
              <p className="text-sm text-foreground/90 font-medium">{t('agentSettings.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('agentSettings.deleteAgentWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button onClick={() => setDeleteConfirm(null)} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('agentSettings.cancel')}</button>
              <button onClick={() => handleDeleteProfile(deleteConfirm)} className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors">{t('agentSettings.delete')}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

// ─── 子智能体编辑表单 ──────────────────────────────────────────

interface ProfileFormProps {
  initial: SubAgentProfile | null
  onSave: (profile: SubAgentProfile) => Promise<void>
  onCancel: () => void
}

// 工具信息接口（从后端 API 动态获取）
interface ToolInfo {
  name: string
  display_name: string
  description: string
}

function ProfileForm({ initial, onSave, onCancel }: ProfileFormProps) {
  const { t } = useTranslation()
  const isEditing = !!initial
  const [saving, setSaving] = useState(false)

  const [id, setId] = useState(initial?.id ?? '')
  const [name, setName] = useState(initial?.name ?? '')
  const [description, setDescription] = useState(initial?.description ?? '')
  const [systemPrompt, setSystemPrompt] = useState(initial?.system_prompt ?? '')
  const [model, setModel] = useState(initial?.model ?? '')
  // 空 enabled_tools 表示"全部工具可用"，前端默认全选
  const [enabledTools, setEnabledTools] = useState<string[]>(
    initial?.enabled_tools && initial.enabled_tools.length > 0
      ? initial.enabled_tools
      : []
  )
  // 工具列表加载后，如果 enabledTools 为空且没有初始值，默认全选
  useEffect(() => {
    if (toolsLoaded && enabledTools.length === 0 && !initial?.enabled_tools?.length) {
      setEnabledTools(allTools.map(t => t.name))
    }
  }, [toolsLoaded, allTools, initial])
  const [maxIterations, setMaxIterations] = useState(initial?.max_iterations ?? 0)
  const [temperature, setTemperature] = useState<number | null>(initial?.temperature ?? null)
  const [compactThreshold, setCompactThreshold] = useState<number | null>(initial?.compact_threshold ?? null)
  const [compactKeep, setCompactKeep] = useState<number | null>(initial?.compact_keep ?? null)
  const promptRef = useRef<HTMLTextAreaElement>(null)

  const [modelOptions, setModelOptions] = useState<string[]>([])
  const [modelOpen, setModelOpen] = useState(false)
  const [allTools, setAllTools] = useState<ToolInfo[]>([])
  const [toolsLoaded, setToolsLoaded] = useState(false)

  // 从后端动态获取工具列表
  useEffect(() => {
    fetch(`${getApiBase()}/tools`).then(r => r.json()).then(body => {
      if (body.success && body.data) {
        setAllTools(body.data)
      }
    }).catch(() => {}).finally(() => setToolsLoaded(true))
  }, [])

  useEffect(() => {
    fetch(`${getApiBase()}/models-config`).then(r => r.json()).then(body => {
      if (body.success && body.data?.providers) {
        const names: string[] = []
        for (const p of body.data.providers) {
          if (p.models) {
            for (const m of p.models) { if (!names.includes(m)) names.push(m) }
          }
        }
        names.sort(); setModelOptions(names)
      }
    }).catch(() => {})
  }, [])

  useEffect(() => {
    if (promptRef.current) {
      promptRef.current.style.height = 'auto'
      promptRef.current.style.height = promptRef.current.scrollHeight + 'px'
    }
  }, [systemPrompt])

  const toggleTool = (n: string) => setEnabledTools(p => p.includes(n) ? p.filter(t => t !== n) : [...p, n])

  const handleSubmit = async () => {
    if (!id.trim() || !name.trim()) return
    setSaving(true)
    try {
      await onSave({
        id: id.trim().toLowerCase().replace(/\s+/g, '-'),
        name: name.trim(), description: description.trim(),
        system_prompt: systemPrompt, model: model || null,
        enabled_tools: enabledTools, max_iterations: maxIterations,
        temperature,
        compact_threshold: compactThreshold,
        compact_keep: compactKeep,
      })
    } finally { setSaving(false) }
  }

  const isDefault = initial?.id === 'default'

  return (
    <div className="space-y-3 rounded-xl border border-border bg-foreground/[0.02] p-4">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium text-foreground/90">
          {isEditing ? t('settings.editEmployee') : t('settings.addEmployee')}
          {isDefault && <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-orange-500/10 text-orange-400">默认智能体</span>}
        </span>
        <button onClick={onCancel} className="p-1 rounded hover:bg-foreground/10"><X className="w-4 h-4 text-foreground/40" /></button>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="block text-[10px] text-foreground/50 mb-1">{t('settings.employeeId')}</label>
          <input value={id} onChange={e => setId(e.target.value)} disabled={isEditing}
            className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none focus:border-foreground/20 disabled:opacity-40" />
        </div>
        <div>
          <label className="block text-[10px] text-foreground/50 mb-1">{t('settings.employeeName')}</label>
          <input value={name} onChange={e => setName(e.target.value)}
            className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none focus:border-foreground/20" />
        </div>
      </div>
      <div>
        <label className="block text-[10px] text-foreground/50 mb-1">{t('settings.employeeDesc')}</label>
        <input value={description} onChange={e => setDescription(e.target.value)}
          className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none focus:border-foreground/20" />
      </div>
      <div>
        <label className="block text-[10px] text-foreground/50 mb-1">SOUL<span className="text-foreground/30 ml-1">（灵魂文件，定义智能体行为）</span></label>
        <textarea ref={promptRef} value={systemPrompt} onChange={e => setSystemPrompt(e.target.value)}
          onInput={() => { const el = promptRef.current; if (el) { el.style.height = 'auto'; el.style.height = el.scrollHeight + 'px' } }}
          rows={3}
          className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs font-mono text-foreground/80 outline-none focus:border-foreground/20 resize-none overflow-hidden" />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="relative">
          <label className="block text-[10px] text-foreground/50 mb-1">{t('settings.employeeModel')}</label>
          <button onClick={() => setModelOpen(!modelOpen)}
            className="w-full flex items-center justify-between gap-1 px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 hover:bg-foreground/10">
            <span className={model ? 'text-foreground/80' : 'text-foreground/40 italic'}>{model || t('settings.inheritModel')}</span>
            <ChevronDown className="w-3 h-3 shrink-0 text-foreground/40" />
          </button>
          {modelOpen && (<>
            <div className="fixed inset-0 z-10" onClick={() => setModelOpen(false)} />
            <div className="absolute bottom-full left-0 mb-1 w-full py-1 rounded-md bg-card border border-border shadow-lg z-20 max-h-40 overflow-y-auto">
              <button className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left text-foreground/50 italic hover:bg-foreground/10"
                onClick={() => { setModel(''); setModelOpen(false) }}>
                {model === '' && <Check className="w-3 h-3 text-emerald-400 shrink-0" />}
                <span className={model === '' ? '' : 'ml-[18px]'}>{t('settings.inheritModel')}</span>
              </button>
              {modelOptions.map(m => (
                <button key={m} className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left text-foreground/70 hover:bg-foreground/10"
                  onClick={() => { setModel(m); setModelOpen(false) }}>
                  {model === m && <Check className="w-3 h-3 text-emerald-400 shrink-0" />}
                  <span className={model === m ? '' : 'ml-[18px]'}>{m}</span>
                </button>
              ))}
            </div>
          </>)}
        </div>
        <div>
          <label className="block text-[10px] text-foreground/50 mb-1">{t('settings.employeeIterations')}</label>
          <input type="number" min={0} max={200} value={maxIterations} onChange={e => setMaxIterations(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none focus:border-foreground/20" />
          <span className="text-[9px] text-foreground/30 mt-0.5 block">{t('settings.employeeIterationsDesc')}</span>
        </div>
      </div>

      {/* 温度设置 */}
      <div>
        <label className="block text-[10px] text-foreground/50 mb-1">生成温度
          <span className="text-foreground/30 ml-1">（留空使用全局默认值 0.70）</span>
        </label>
        <div className="flex items-center gap-3">
          <input
            type="range" min={0} max={200} step={5}
            value={temperature !== null ? Math.round(temperature * 100) : 70}
            onChange={e => setTemperature(parseInt(e.target.value) / 100)}
            className="flex-1 h-1.5 bg-foreground/10 rounded-lg appearance-none cursor-pointer accent-cyan-500"
          />
          <span className="text-xs text-foreground/70 font-mono w-16 text-right">
            {temperature !== null ? temperature.toFixed(2) : '默认'}
          </span>
          {temperature !== null && (
            <button onClick={() => setTemperature(null)}
              className="text-[10px] px-1.5 py-0.5 rounded hover:bg-foreground/10 text-foreground/40">
              重置
            </button>
          )}
        </div>
      </div>

      {/* 上下文压缩设置 */}
      <div>
        <label className="block text-[10px] text-foreground/50 mb-1">上下文压缩
          <span className="text-foreground/30 ml-1">（留空使用全局默认值）</span>
        </label>
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-[10px] text-foreground/50">超过</span>
          <input
            type="number" min={0} max={999} placeholder="40"
            value={compactThreshold ?? ''}
            onChange={e => setCompactThreshold(e.target.value ? parseInt(e.target.value) : null)}
            className="w-16 px-2 py-1.5 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none focus:border-foreground/20"
          />
          <span className="text-[10px] text-foreground/50">条消息时压缩，保留</span>
          <input
            type="number" min={1} max={200} placeholder="20"
            value={compactKeep ?? ''}
            onChange={e => setCompactKeep(e.target.value ? parseInt(e.target.value) : null)}
            className="w-16 px-2 py-1.5 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 outline-none focus:border-foreground/20"
          />
          <span className="text-[10px] text-foreground/50">条</span>
          {(compactThreshold !== null || compactKeep !== null) && (
            <button onClick={() => { setCompactThreshold(null); setCompactKeep(null) }}
              className="text-[10px] px-1.5 py-0.5 rounded hover:bg-foreground/10 text-foreground/40">
              重置
            </button>
          )}
        </div>
      </div>

      <div>
        <label className="block text-[10px] text-foreground/50 mb-1">{t('settings.employeeTools')}</label>
        <div className="rounded-lg bg-foreground/5 border border-border overflow-hidden">
          {!toolsLoaded ? (
            <div className="text-xs text-foreground/40 py-3 px-3">{t('settings.employeeToolsLoading')}</div>
          ) : allTools.length === 0 ? (
            <div className="text-xs text-foreground/40 py-3 px-3">暂无可用工具</div>
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-border bg-foreground/[0.03]">
                  <th className="w-10 px-3 py-2 text-left">
                    <input
                      type="checkbox"
                      checked={allTools.length > 0 && enabledTools.length === allTools.length}
                      onChange={() => {
                        if (enabledTools.length === allTools.length) setEnabledTools([])
                        else setEnabledTools(allTools.map(t => t.name))
                      }}
                      className="w-3.5 h-3.5 rounded border-foreground/30 accent-amber-500"
                    />
                  </th>
                  <th className="px-2 py-2 text-left text-foreground/60 font-medium">{t('settings.employeeToolsName')}</th>
                  <th className="px-2 py-2 text-left text-foreground/60 font-medium hidden sm:table-cell">{t('settings.employeeToolsDesc')}</th>
                </tr>
              </thead>
              <tbody>
                {allTools.map(tool => {
                  const sel = enabledTools.includes(tool.name)
                  return (
                    <tr key={tool.name} onClick={() => toggleTool(tool.name)}
                      className={`border-b border-border/50 last:border-0 cursor-pointer transition-colors ${sel ? 'bg-amber-500/8' : 'hover:bg-foreground/[0.02]'}`}>
                      <td className="px-3 py-2">
                        <input type="checkbox" checked={sel} onChange={() => toggleTool(tool.name)}
                          className="w-3.5 h-3.5 rounded border-foreground/30 accent-amber-500" />
                      </td>
                      <td className={`px-2 py-2 font-medium ${sel ? 'text-amber-600 dark:text-amber-300' : 'text-foreground/70'}`}>{tool.display_name || tool.name}</td>
                      <td className="px-2 py-2 text-foreground/40 truncate max-w-[200px] hidden sm:table-cell">{tool.description}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
          <div className="flex items-center justify-between px-3 py-2 border-t border-border">
            <span className="text-[10px] text-foreground/30">
              {enabledTools.length === 0 ? t('settings.employeeToolsAll') : t('settings.employeeToolsCount', { n: enabledTools.length, total: allTools.length })}
            </span>
            {enabledTools.length > 0 && (
              <button onClick={() => setEnabledTools([])} className="text-[10px] text-red-400/60 hover:text-red-400">{t('settings.employeeToolsClear')}</button>
            )}
          </div>
        </div>
      </div>
      <div className="flex items-center gap-2 pt-1">
        <button onClick={handleSubmit} disabled={saving || !id.trim() || !name.trim()}
          className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium">{saving ? t('settings.saving') : isEditing ? t('settings.save') : t('settings.add')}</button>
        <button onClick={onCancel} className="px-4 py-1.5 rounded-lg bg-foreground/5 hover:bg-foreground/10 text-xs text-foreground/60">{t('settings.cancel')}</button>
      </div>
    </div>
  )
}
