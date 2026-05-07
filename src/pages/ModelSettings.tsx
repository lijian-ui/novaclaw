import { useState, useEffect, useCallback } from 'react'
import { Plus, Play, Trash2, Pencil, Star, ChevronRight, ChevronDown, X, Check, ArrowLeft } from 'lucide-react'
import { useApi } from '@/hooks/useApi'
import { useTauriCommands } from '@/hooks/useTauriCommands'
import type { ProviderConfig } from '@/types'

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

interface ModelsConfig {
  default_model?: string
  providers: ProviderConfig[]
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
  const { isTauri, getModelsConfig, saveModelsConfig } = useTauriCommands()
  const { listProviders, saveProvider, testConnection, setDefaultModel, getDefaultModel } = useApi()

  const [providers, setProviders] = useState<Provider[]>([])
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set(['deepseek', 'qwen', 'openai']))
  const [showModal, setShowModal] = useState(false)
  const [editingModel, setEditingModel] = useState<string | null>(null)
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const [testStatus, setTestStatus] = useState<'idle' | 'testing' | 'success' | 'fail'>('idle')
  const [testMessage, setTestMessage] = useState<string>('')

  const [saveError, setSaveError] = useState<string | null>(null)

  const loadProviders = useCallback(async () => {
    try {
      let apiProviders: ProviderConfig[] = []
      let defaultModelName = ''
      if (isTauri) {
        const config = await getModelsConfig() as unknown as ModelsConfig
        apiProviders = config.providers || []
        defaultModelName = config.default_model || ''
      } else {
        // 通过 listProviders 获取 providers，通过 getDefaultModel 获取默认模型
        apiProviders = await listProviders()
        defaultModelName = await getDefaultModel()
      }
      if (apiProviders.length > 0) {
        const localProviders = apiProvidersToLocal(apiProviders)
        // 标记默认模型
        if (defaultModelName) {
          for (const provider of localProviders) {
            for (const model of provider.models) {
              model.isDefault = model.name === defaultModelName
            }
          }
        }
        setProviders(localProviders)
      }
    } catch (err) {
      console.error('[ModelSettings] 加载提供商失败:', err)
    }
  }, [isTauri, getModelsConfig, listProviders, getDefaultModel])

  useEffect(() => {
    loadProviders()
  }, [loadProviders])

  const persist = useCallback(async (newProviders: Provider[]) => {
    setProviders(newProviders)
    setSaveError(null)
    const apiData = localToApiProviders(newProviders)
    
    // 获取当前的默认模型名称
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
    
    console.log('[ModelSettings] 保存提供商:', JSON.stringify(apiData), '默认模型:', defaultModelName)
    try {
      if (isTauri) {
        await saveModelsConfig({ 
          default_model: defaultModelName,
          providers: apiData 
        })
      } else {
        await saveProvider(apiData)
      }
      console.log('[ModelSettings] 保存成功')
    } catch (err) {
      const msg = err instanceof Error ? err.message : '保存失败，请检查后端服务'
      console.error('[ModelSettings] 保存失败:', err)
      setSaveError(msg)
      throw err
    }
  }, [isTauri, saveModelsConfig, saveProvider])

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
      setTestMessage('请先填写 API Key')
      return
    }
    if (!form.baseUrl.trim() || form.baseUrl === 'https://') {
      setTestStatus('fail')
      setTestMessage('请先填写 Base URL')
      return
    }
    if (!form.name.trim()) {
      setTestStatus('fail')
      setTestMessage('请先填写模型名称')
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
      setTestMessage(result.message || (result.success ? '连接成功' : '连接失败'))
    }).catch(() => {
      setTestStatus('fail')
      setTestMessage('后端服务未运行或网络错误')
    })
  }

  const handleSave = async () => {
    if (!form.name.trim()) return

    const updated = [...providers]
    let providerIdx = updated.findIndex(p => p.id === form.provider)

    // 如果供应商不存在，先创建
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
    // 找到被设为默认的模型名称
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
    // 保存到本地状态和后端配置
    persist(providers.map(p => ({
      ...p,
      models: p.models.map(m => ({ ...m, isDefault: m.id === modelId })),
    })))
    if (defaultModelName) {
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
    ollama: 'text-amber-400',
    lmstudio: 'text-cyan-400',
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">模型配置</span>
        </div>
        <button
          onClick={() => openAddModal()}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 text-xs transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          添加模型
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {providers.filter(p => p.models.length > 0).map((provider) => {
          const isExpanded = expandedIds.has(provider.id)
          const defaultModel = provider.models.find(m => m.isDefault)

          return (
            <div
              key={provider.id}
              className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors"
            >
              {/* Provider header */}
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
                      默认: {defaultModel.name}
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-foreground/30">{provider.models.length} 模型</span>
                  <button
                    onClick={() => openAddModal(provider.id)}
                    className="p-1 rounded hover:bg-foreground/10 transition-colors"
                  >
                    <Plus className="w-3.5 h-3.5 text-foreground/40" />
                  </button>
                </div>
              </div>

              {/* Model list */}
              {isExpanded && (
                <div className="px-3 pb-3 space-y-1.5">
                  {provider.models.length === 0 ? (
                    <div className="px-3 py-4 text-center">
                      <p className="text-xs text-foreground/30">暂无模型，点击 + 添加</p>
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
                              title="设为默认"
                            >
                              <Star className="w-3 h-3 text-foreground/30 hover:text-yellow-400" />
                            </button>
                          )}
                          <button
                            onClick={() => openEditModal(model)}
                            className="p-1 rounded hover:bg-foreground/10 transition-colors"
                            title="编辑"
                          >
                            <Pencil className="w-3 h-3 text-foreground/40" />
                          </button>
                          <button
                            onClick={() => toggleEnabled(model.id)}
                            className={`relative w-7 h-3.5 rounded-full transition-colors mx-1 ${
                              model.enabled ? 'bg-green-500' : 'bg-foreground/20'
                            }`}
                            title={model.enabled ? '禁用' : '启用'}
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
                            title="删除"
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

      {/* Add/Edit Modal */}
      {showModal && (
        <>
          <div className="fixed inset-0 bg-black/50 z-30" onClick={() => setShowModal(false)} />
          <div className="fixed inset-0 z-40 flex items-center justify-center p-4">
            <div className="w-full max-w-md rounded-xl border border-border bg-card shadow-2xl">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground/90">
                  {editingModel ? '编辑模型' : '添加模型'}
                </span>
                <button onClick={() => setShowModal(false)} className="p-1 rounded hover:bg-foreground/10 transition-colors">
                  <X className="w-4 h-4 text-foreground/50" />
                </button>
              </div>

              <div className="px-4 py-4 space-y-3">
                {/* Provider */}
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">供应商 *</label>
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

                {/* Model name */}
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">模型名称 *</label>
                  <input
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder="例如: gpt-4o / deepseek-chat"
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                  />
                </div>

                {/* API Key */}
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">API Key *</label>
                  <input
                    value={form.apiKey}
                    onChange={e => setForm(f => ({ ...f, apiKey: e.target.value }))}
                    type="password"
                    placeholder="sk-..."
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                  />
                </div>

                {/* Base URL */}
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">Base URL *</label>
                  <input
                    value={form.baseUrl}
                    onChange={e => setForm(f => ({ ...f, baseUrl: e.target.value }))}
                    placeholder="https://api.example.com"
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                  />
                </div>
              </div>

              {/* Error message */}
              {saveError && (
                <div className="px-4 pb-2">
                  <div className="px-3 py-1.5 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-400">
                    {saveError}
                  </div>
                </div>
              )}

              {/* 测试结果消息 */}
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

              {/* Modal footer */}
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
                      测试中...
                    </>
                  ) : testStatus === 'success' ? (
                    <>
                      <Check className="w-3 h-3" />
                      连接成功
                    </>
                  ) : (
                    <>
                      <Play className="w-3 h-3" />
                      测试连接
                    </>
                  )}
                </button>

                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setShowModal(false)}
                    className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
                  >
                    取消
                  </button>
                  <button
                    onClick={handleSave}
                    disabled={testStatus === 'testing'}
                    className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium transition-colors"
                  >
                    保存
                  </button>
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Delete confirmation */}
      {showDeleteConfirm && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="w-full max-w-sm rounded-xl border border-border bg-card shadow-2xl">
            <div className="px-4 py-4">
              <p className="text-sm text-foreground/90 font-medium">确认删除</p>
              <p className="text-xs text-foreground/50 mt-2">确定要删除此模型吗？此操作不可撤销。</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button
                onClick={() => setShowDeleteConfirm(null)}
                className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
              >
                取消
              </button>
              <button
                onClick={confirmDelete}
                className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors"
              >
                删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
