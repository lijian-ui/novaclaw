import { useState, useEffect, useCallback } from 'react'
import { Plus, Play, Trash2, Pencil, Star, ChevronRight, ChevronDown, X, Check, ArrowLeft } from 'lucide-react'
import { useApi } from '@/hooks/useApi'
import { useChat } from '@/contexts/ChatContext'
import type { ProviderConfig } from '@/types'
import { useTranslation } from 'react-i18next'

interface ModelConfig {
  id: string
  name: string
  provider: string
  apiKey: string
  baseUrl: string
  enabled: boolean
  isDefault: boolean
}

interface Provider {
  id: string
  name: string
  models: ModelConfig[]
}

function apiProvidersToLocal(apiProviders: ProviderConfig[]): Provider[] {
  return apiProviders.map((p) => ({
    id: p.name.toLowerCase().replace(/[\s_-]/g, ''),
    name: p.name,
    models: p.models.map((m, i) => ({
      id: `${p.name.toLowerCase().replace(/[\s_-]/g, '')}_${i}`,
      name: m,
      provider: p.name,
      apiKey: p.api_key,
      baseUrl: p.base_url,
      enabled: true,
      isDefault: i === 0,
    })),
  }))
}

function localToApiProviders(providers: Provider[]): ProviderConfig[] {
  const seen = new Map<string, ProviderConfig>()
  for (const p of providers) {
    const existing = seen.get(p.id) || { name: p.name, api_key: '', base_url: '', models: [] }
    for (const m of p.models) {
      existing.api_key = m.apiKey || existing.api_key
      existing.base_url = m.baseUrl || existing.base_url
      if (!existing.models.includes(m.name)) {
        existing.models.push(m.name)
      }
    }
    seen.set(p.id, existing)
  }
  return Array.from(seen.values())
}

const supplierOptions = [
  { id: 'deepseek', name: 'DeepSeek' },
  { id: 'qwen', name: 'Qwen' },
  { id: 'openai', name: 'OpenAI' },
  { id: 'anthropic', name: 'Anthropic' },
  { id: 'zhipu', name: '智谱AI' },
  { id: 'ollama', name: 'Ollama' },
  { id: 'lmstudio', name: 'LM Studio' },
]

const emptyForm = {
  name: '',
  provider: '',
  apiKey: '',
  baseUrl: '',
}

interface ModelSettingsProps {
  onBack?: () => void
}

export function ModelSettings({ onBack }: ModelSettingsProps) {
  const { t } = useTranslation()
  const chat = useChat()
  const { listProviders, saveProvider, testConnection, setDefaultModel, getDefaultModel } = useApi()

  const [providers, setProviders] = useState<Provider[]>([])
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set(['deepseek', 'qwen', 'openai', 'anthropic', 'zhipu']))
  const [showModal, setShowModal] = useState(false)
  const [editingModel, setEditingModel] = useState<string | null>(null)
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const [testStatus, setTestStatus] = useState<'idle' | 'testing' | 'success' | 'fail'>('idle')
  const [testMessage, setTestMessage] = useState<string>('')

  const [saveError, setSaveError] = useState<string | null>(null)

  const loadProviders = useCallback(async () => {
    try {
      const apiProviders = await listProviders()
      const defaultModelName = await getDefaultModel()
      if (apiProviders.length > 0) {
        const localProviders = apiProvidersToLocal(apiProviders)
        if (defaultModelName) {
          chat.setDefaultModelName(defaultModelName)
          for (const provider of localProviders) {
            for (const model of provider.models) {
              model.isDefault = model.name === defaultModelName
            }
          }
        }
        setProviders(localProviders)
      }
    } catch (err) {
      console.error('[ModelSettings]', t('modelSettings.loadingProviders'), err)
    }
  }, [listProviders, getDefaultModel, t])

  useEffect(() => {
    loadProviders()
  }, [loadProviders])

  const persist = useCallback(async (newProviders: Provider[]) => {
    setProviders(newProviders)
    setSaveError(null)
    const apiData = localToApiProviders(newProviders)
    
    let defaultModelName = ''
    for (const provider of newProviders) {
      for (const model of provider.models) {
        if (model.isDefault) {
          defaultModelName = model.name
          break
        }
      }
      if (defaultModelName) break
    }
    
    console.log('[ModelSettings]', t('modelSettings.savingProviders'), JSON.stringify(apiData), t('settings.modelTitle') + ':', defaultModelName)
    try {
      await saveProvider(apiData)
      console.log('[ModelSettings]', t('modelSettings.saveSuccessLog'))
    } catch (err) {
      const msg = err instanceof Error ? err.message : t('modelSettings.saveFailed')
      console.error('[ModelSettings]', t('modelSettings.saveFailedLog'), err)
      setSaveError(msg)
      throw err
    }
  }, [saveProvider, t])

  const toggleExpanded = (id: string) => {
    setExpandedIds(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const openAddModal = (providerId?: string) => {
    setEditingModel(null)
    setForm({ ...emptyForm, provider: providerId || 'deepseek' })
    setTestStatus('idle')
    setTestMessage('')
    setShowModal(true)
  }

  const openEditModal = (model: ModelConfig) => {
    setEditingModel(model.id)
    setForm({ name: model.name, provider: model.provider, apiKey: model.apiKey, baseUrl: model.baseUrl })
    setTestStatus('idle')
    setTestMessage('')
    setShowModal(true)
  }

  const handleTest = () => {
    if (!form.apiKey.trim()) {
      setTestStatus('fail')
      setTestMessage(t('modelSettings.pleaseFillApiKey'))
      return
    }
    if (!form.baseUrl.trim() || form.baseUrl === 'https://') {
      setTestStatus('fail')
      setTestMessage(t('modelSettings.pleaseFillBaseUrl'))
      return
    }
    if (!form.name.trim()) {
      setTestStatus('fail')
      setTestMessage(t('modelSettings.pleaseFillModelName'))
      return
    }
    setTestStatus('testing')
    setTestMessage('')
    testConnection({
      api_key: form.apiKey,
      base_url: form.baseUrl.replace(/\/+$/, ''),
      model: form.name,
    }).then((result) => {
      setTestStatus(result.success ? 'success' : 'fail')
      setTestMessage(result.message || (result.success ? t('modelSettings.connectionSuccess') : t('modelSettings.connectionFailed')))
    }).catch(() => {
      setTestStatus('fail')
      setTestMessage(t('modelSettings.backendNotRunning'))
    })
  }

  const handleSave = async () => {
    if (!form.name.trim()) return

    const updated = [...providers]
    let providerIdx = updated.findIndex(p => p.id === form.provider)

    if (providerIdx === -1) {
      const supplier = supplierOptions.find(s => s.id === form.provider)
      if (!supplier) return
      updated.push({
        id: form.provider,
        name: supplier.name,
        models: [],
      })
      providerIdx = updated.length - 1
    }

    const provider = updated[providerIdx]
    const newModel: ModelConfig = {
      id: editingModel || `model_${Date.now()}`,
      name: form.name,
      provider: form.provider,
      apiKey: form.apiKey,
      baseUrl: form.baseUrl,
      enabled: true,
      isDefault: false,
    }

    if (editingModel) {
      provider.models = provider.models.map(m => m.id === editingModel ? newModel : m)
    } else {
      provider.models = [...provider.models, newModel]
    }

    updated[providerIdx] = provider
    try {
      await persist(updated)
      setShowModal(false)
    } catch {
      // persist() 已经设置了 saveError 状态，模态框保持打开让用户看到错误
    }
  }

  const toggleEnabled = (modelId: string) => {
    persist(providers.map(p => ({
      ...p,
      models: p.models.map(m => m.id === modelId ? { ...m, enabled: !m.enabled } : m),
    })))
  }

  const setDefault = (modelId: string) => {
    let defaultModelName = ''
    for (const p of providers) {
      for (const m of p.models) {
        if (m.id === modelId) {
          defaultModelName = m.name
          break
        }
      }
      if (defaultModelName) break
    }
    persist(providers.map(p => ({
      ...p,
      models: p.models.map(m => ({ ...m, isDefault: m.id === modelId })),
    })))
    if (defaultModelName) {
      chat.setDefaultModelName(defaultModelName)
      setDefaultModel(defaultModelName).catch(() => {})
    }
  }

  const confirmDelete = () => {
    if (showDeleteConfirm) {
      persist(providers.map(p => ({
        ...p,
        models: p.models.filter(m => m.id !== showDeleteConfirm),
      })))
      setShowDeleteConfirm(null)
    }
  }

  const providerColors: Record<string, string> = {
    deepseek: 'text-blue-400',
    qwen: 'text-violet-400',
    openai: 'text-emerald-400',
    anthropic: 'text-orange-400',
    zhipu: 'text-red-400',
    ollama: 'text-amber-400',
    lmstudio: 'text-cyan-400',
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('modelSettings.title')}</span>
        </div>
        <button
          onClick={() => openAddModal()}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 text-white text-xs font-medium transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          {t('modelSettings.addModel')}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {providers.filter(p => p.models.length > 0).map((provider) => {
          const isExpanded = expandedIds.has(provider.id)
          const defaultModel = provider.models.find(m => m.isDefault)

          return (
            <div
              key={provider.id}
              className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors"
            >
              <div className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => toggleExpanded(provider.id)}
                    className="p-0.5 rounded hover:bg-foreground/10 transition-colors"
                  >
                    {isExpanded ? (
                      <ChevronDown className="w-4 h-4 text-foreground/40" />
                    ) : (
                      <ChevronRight className="w-4 h-4 text-foreground/40" />
                    )}
                  </button>
                  <span className={`text-sm font-semibold ${providerColors[provider.id] || 'text-foreground/80'}`}>
                    {provider.name}
                  </span>
                  {defaultModel && (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400">
                      {t('modelSettings.default', { model: defaultModel.name })}
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-foreground/30">{t('modelSettings.modelsCount', { count: provider.models.length })}</span>
                  <button
                    onClick={() => openAddModal(provider.id)}
                    className="p-1 rounded hover:bg-foreground/10 transition-colors"
                  >
                    <Plus className="w-3.5 h-3.5 text-foreground/40" />
                  </button>
                </div>
              </div>

              {isExpanded && (
                <div className="px-3 pb-3 space-y-1.5">
                  {provider.models.length === 0 ? (
                    <div className="px-3 py-4 text-center">
                      <p className="text-xs text-foreground/30">{t('modelSettings.noModels')}</p>
                    </div>
                  ) : (
                    provider.models.map((model) => (
                      <div
                        key={model.id}
                        className="flex items-center justify-between px-3 py-2 rounded-lg bg-foreground/[0.03] hover:bg-foreground/[0.06] transition-colors"
                      >
                        <div className="flex items-center gap-2.5 min-w-0">
                          <div
                            className={`p-1 rounded ${model.enabled ? 'bg-green-500/10' : 'bg-foreground/5'}`}
                          >
                            <div className={`w-2 h-2 rounded-full ${model.enabled ? 'bg-green-400' : 'bg-foreground/20'}`} />
                          </div>
                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span className="text-xs text-foreground/80 font-mono truncate">{model.name}</span>
                              {model.isDefault && (
                                <Star className="w-3 h-3 text-yellow-400 fill-yellow-400 shrink-0" />
                              )}
                            </div>
                            <p className="text-[10px] text-foreground/30 truncate">{model.baseUrl}</p>
                          </div>
                        </div>

                        <div className="flex items-center gap-0.5 shrink-0">
                          {!model.isDefault && (
                            <button
                              onClick={() => setDefault(model.id)}
                              className="p-1 rounded hover:bg-blue-500/10 transition-colors"
                              title={t('modelSettings.setDefault')}
                            >
                              <Star className="w-3 h-3 text-foreground/30 hover:text-yellow-400" />
                            </button>
                          )}
                          <button
                            onClick={() => openEditModal(model)}
                            className="p-1 rounded hover:bg-foreground/10 transition-colors"
                            title={t('modelSettings.edit')}
                          >
                            <Pencil className="w-3 h-3 text-foreground/40" />
                          </button>
                          <button
                            onClick={() => toggleEnabled(model.id)}
                            className={`relative w-7 h-3.5 rounded-full transition-colors mx-1 ${
                              model.enabled ? 'bg-green-500' : 'bg-foreground/20'
                            }`}
                            title={model.enabled ? t('modelSettings.disable') : t('modelSettings.enable')}
                          >
                            <div
                              className={`absolute top-0.5 w-2.5 h-2.5 rounded-full bg-foreground transition-transform ${
                                model.enabled ? 'translate-x-3.5' : 'translate-x-0.5'
                              }`}
                            />
                          </button>
                          <button
                            onClick={() => setShowDeleteConfirm(model.id)}
                            className="p-1 rounded hover:bg-red-500/10 transition-colors"
                            title={t('modelSettings.delete')}
                          >
                            <Trash2 className="w-3 h-3 text-foreground/40 hover:text-red-400" />
                          </button>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              )}
            </div>
          )
        })}
      </div>

      {showModal && (
        <>
          <div className="fixed inset-0 bg-black/50 z-30" onClick={() => setShowModal(false)} />
          <div className="fixed inset-0 z-40 flex items-center justify-center p-4">
            <div className="w-full max-w-md rounded-xl border border-border bg-card shadow-2xl">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground/90">
                  {editingModel ? t('modelSettings.editModel') : t('modelSettings.addModel')}
                </span>
                <button onClick={() => setShowModal(false)} className="p-1 rounded hover:bg-foreground/10 transition-colors">
                  <X className="w-4 h-4 text-foreground/50" />
                </button>
              </div>

              <div className="px-4 py-4 space-y-3">
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('modelSettings.provider')}</label>
                  <div className="relative">
                    <select
                      value={form.provider}
                      onChange={e => setForm(f => ({ ...f, provider: e.target.value }))}
                      className="w-full appearance-none px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 outline-none focus:border-foreground/20 transition-colors cursor-pointer"
                    >
                      {supplierOptions.map((s) => (
                        <option key={s.id} value={s.id} className="bg-card text-foreground/80">
                          {s.name}
                        </option>
                      ))}
                    </select>
                    <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-foreground/40 pointer-events-none" />
                  </div>
                </div>

                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('modelSettings.modelName')}</label>
                  <input
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder="例如: gpt-4o / deepseek-chat"
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                  />
                </div>

                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('modelSettings.apiKey')}</label>
                  <input
                    value={form.apiKey}
                    onChange={e => setForm(f => ({ ...f, apiKey: e.target.value }))}
                    type="password"
                    placeholder="sk-..."
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                  />
                </div>

                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('modelSettings.baseUrl')}</label>
                  <input
                    value={form.baseUrl}
                    onChange={e => setForm(f => ({ ...f, baseUrl: e.target.value }))}
                    placeholder="https://api.example.com"
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                  />
                </div>
              </div>

              {saveError && (
                <div className="px-4 pb-2">
                  <div className="px-3 py-1.5 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-400">
                    {saveError}
                  </div>
                </div>
              )}

              {testMessage && (
                <div className={`px-4 pb-0 ${testStatus === 'success' ? '' : ''}`}>
                  <div className={`px-3 py-1.5 rounded-lg text-xs ${
                    testStatus === 'success'
                      ? 'bg-green-500/10 border border-green-500/20 text-green-400'
                      : 'bg-red-500/10 border border-red-500/20 text-red-400'
                  }`}>
                    {testMessage}
                  </div>
                </div>
              )}

              <div className="flex items-center justify-between px-4 py-3 border-t border-border">
                <button
                  onClick={handleTest}
                  disabled={testStatus === 'testing'}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs transition-colors ${
                    testStatus === 'success'
                      ? 'bg-green-500/20 text-green-400'
                      : testStatus === 'fail'
                      ? 'bg-red-500/20 text-red-400'
                      : 'bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
                  }`}
                >
                  {testStatus === 'testing' ? (
                    <>
                      <div className="w-3 h-3 border-2 border-foreground/30 border-t-foreground rounded-full animate-spin" />
                      {t('modelSettings.testing')}
                    </>
                  ) : testStatus === 'success' ? (
                    <>
                      <Check className="w-3 h-3" />
                      {t('modelSettings.connectionSuccess')}
                    </>
                  ) : (
                    <>
                      <Play className="w-3 h-3" />
                      {t('modelSettings.testConnection')}
                    </>
                  )}
                </button>

                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setShowModal(false)}
                    className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
                  >
                    {t('modelSettings.cancel')}
                  </button>
                  <button
                    onClick={handleSave}
                    disabled={testStatus === 'testing'}
                    className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium transition-colors"
                  >
                    {t('modelSettings.save')}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {showDeleteConfirm && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="w-full max-w-sm rounded-xl border border-border bg-card shadow-2xl">
            <div className="px-4 py-4">
              <p className="text-sm text-foreground/90 font-medium">{t('modelSettings.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('modelSettings.deleteModelWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button
                onClick={() => setShowDeleteConfirm(null)}
                className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
              >
                {t('modelSettings.cancel')}
              </button>
              <button
                onClick={confirmDelete}
                className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors"
              >
                {t('modelSettings.delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
