import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, Pencil, ChevronRight, ChevronDown, X, ArrowLeft, Shield, Webhook } from 'lucide-react'
import { API_BASE } from '@/hooks/useApi'
import { useTranslation } from 'react-i18next'

interface IMChannel {
  id: string
  name: string
  enabled: boolean
  config: {
    webhook?: string
    secret?: string
    clientId?: string
    clientSecret?: string
    appId?: string
    appSecret?: string
    agentId?: string
    corpId?: string
  }
}

interface IMSettingsProps {
  onBack?: () => void
}

const channelTypes = [
  { id: 'dingtalk', name: '钉钉', icon: '🔔', color: 'text-blue-400' },
  { id: 'feishu', name: '飞书', icon: '📮', color: 'text-green-400' },
]

const emptyForm = {
  webhook: '',
  secret: '',
  clientId: '',
  clientSecret: '',
  appId: '',
  appSecret: '',
  agentId: '',
  corpId: '',
}

export function IMSettings({ onBack }: IMSettingsProps) {
  const { t } = useTranslation()

  const [channels, setChannels] = useState<IMChannel[]>([])
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set())
  const [showModal, setShowModal] = useState(false)
  const [editingChannel, setEditingChannel] = useState<string | null>(null)
  const [selectedChannelType, setSelectedChannelType] = useState('dingtalk')
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)

  useEffect(() => {
    // 初始化配置（IM 设置使用配置目录）
    fetch(`${API_BASE}/paths`).then(r => r.json()).catch(() => {})
  }, [])

  const loadChannels = useCallback(async () => {
    try {
      const response = await fetch(`${API_BASE}/config/im_channels`)
      if (response.ok) {
        const data = await response.json()
        setChannels(data.channels || [])
      }
    } catch (err) {
      console.error('[IMSettings] 加载配置失败', err)
      setChannels([])
    }
  }, [])

  useEffect(() => {
    loadChannels()
  }, [loadChannels])

  const persist = useCallback(async (newChannels: IMChannel[]) => {
    setChannels(newChannels)
    setSaveError(null)
    try {
      const response = await fetch(`${API_BASE}/config/im_channels`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ channels: newChannels }),
      })
      if (!response.ok) {
        throw new Error('保存失败')
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : '保存失败'
      console.error('[IMSettings] 保存失败', err)
      setSaveError(msg)
      throw err
    }
  }, [])

  const toggleExpanded = (id: string) => {
    setExpandedIds(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const openAddModal = (channelType?: string) => {
    setEditingChannel(null)
    setSelectedChannelType(channelType || 'dingtalk')
    setForm(emptyForm)
    setShowModal(true)
  }

  const openEditModal = (channel: IMChannel) => {
    setEditingChannel(channel.id)
    setSelectedChannelType(channel.id)
    setForm({ ...emptyForm, ...channel.config })
    setShowModal(true)
  }

  const handleSave = async () => {
    const selectedType = channelTypes.find(c => c.id === selectedChannelType)
    if (!selectedType) return

    const newChannel: IMChannel = {
      id: editingChannel || `im_${Date.now()}`,
      name: selectedType.name,
      enabled: true,
      config: form,
    }

    let updated: IMChannel[]
    if (editingChannel) {
      updated = channels.map(c => c.id === editingChannel ? newChannel : c)
    } else {
      if (channels.some(c => c.id === selectedChannelType)) {
        setSaveError('该渠道已存在')
        return
      }
      updated = [...channels, newChannel]
    }

    try {
      await persist(updated)
      setShowModal(false)
    } catch {
      // persist() 已经设置了 saveError 状态
    }
  }

  const toggleEnabled = (channelId: string) => {
    persist(channels.map(c => c.id === channelId ? { ...c, enabled: !c.enabled } : c))
  }

  const confirmDelete = () => {
    if (showDeleteConfirm) {
      persist(channels.filter(c => c.id !== showDeleteConfirm))
      setShowDeleteConfirm(null)
    }
  }

  const channelColors: Record<string, string> = {
    dingtalk: 'text-blue-400',
    feishu: 'text-green-400',
  }

  const getChannelIcon = (id: string) => {
    const channel = channelTypes.find(c => c.id === id)
    return channel?.icon || '💬'
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('imSettings.title')}</span>
        </div>
        <button
          onClick={() => openAddModal()}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 text-xs transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          {t('imSettings.addChannel')}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {channels.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <div className="w-16 h-16 rounded-full bg-foreground/5 flex items-center justify-center mb-4">
              <Webhook className="w-8 h-8 text-foreground/20" />
            </div>
            <p className="text-sm text-foreground/40 mb-1">{t('imSettings.noChannels')}</p>
            <p className="text-xs text-foreground/30">{t('imSettings.addChannelHint')}</p>
          </div>
        ) : (
          channels.map((channel) => {
            const isExpanded = expandedIds.has(channel.id)

            return (
              <div
                key={channel.id}
                className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors"
              >
                <div className="flex items-center justify-between px-4 py-3">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => toggleExpanded(channel.id)}
                      className="p-0.5 rounded hover:bg-foreground/10 transition-colors"
                    >
                      {isExpanded ? (
                        <ChevronDown className="w-4 h-4 text-foreground/40" />
                      ) : (
                        <ChevronRight className="w-4 h-4 text-foreground/40" />
                      )}
                    </button>
                    <span className="text-lg">{getChannelIcon(channel.id)}</span>
                    <div className="flex items-center gap-2">
                      <span className={`text-sm font-semibold ${channelColors[channel.id] || 'text-foreground/80'}`}>
                        {channel.name}
                      </span>
                      {channel.enabled && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/10 text-green-400">
                          {t('imSettings.enabled')}
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => openEditModal(channel)}
                      className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
                      title={t('imSettings.edit')}
                    >
                      <Pencil className="w-3.5 h-3.5 text-foreground/40" />
                    </button>
                    <button
                      onClick={() => toggleEnabled(channel.id)}
                      className={`relative w-9 h-5 rounded-full transition-colors ${
                        channel.enabled ? 'bg-green-500' : 'bg-foreground/20'
                      }`}
                      title={channel.enabled ? t('imSettings.disable') : t('imSettings.enable')}
                    >
                      <div
                        className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                          channel.enabled ? 'translate-x-4.5' : 'translate-x-0.5'
                        }`}
                      />
                    </button>
                    <button
                      onClick={() => setShowDeleteConfirm(channel.id)}
                      className="p-1.5 rounded hover:bg-red-500/10 transition-colors"
                      title={t('imSettings.delete')}
                    >
                      <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                    </button>
                  </div>
                </div>

                {isExpanded && (
                  <div className="px-4 pb-4 border-t border-border/50">
                    <div className="pt-3 space-y-2">
                      {channel.config.webhook && (
                        <div className="flex items-start gap-2">
                          <Webhook className="w-3.5 h-3.5 text-foreground/30 mt-0.5" />
                          <div className="min-w-0">
                            <p className="text-[10px] text-foreground/40">{t('imSettings.webhook')}</p>
                            <p className="text-xs text-foreground/60 font-mono truncate">{channel.config.webhook}</p>
                          </div>
                        </div>
                      )}
                      {channel.config.clientId && (
                        <div className="flex items-start gap-2">
                          <Shield className="w-3.5 h-3.5 text-foreground/30 mt-0.5" />
                          <div className="min-w-0">
                            <p className="text-[10px] text-foreground/40">Client ID</p>
                            <p className="text-xs text-foreground/60 font-mono truncate">{channel.config.clientId}</p>
                          </div>
                        </div>
                      )}
                      {channel.config.corpId && (
                        <div className="flex items-start gap-2">
                          <Shield className="w-3.5 h-3.5 text-foreground/30 mt-0.5" />
                          <div className="min-w-0">
                            <p className="text-[10px] text-foreground/40">{t('imSettings.corpId')}</p>
                            <p className="text-xs text-foreground/60 font-mono truncate">{channel.config.corpId}</p>
                          </div>
                        </div>
                      )}
                      {channel.config.agentId && (
                        <div className="flex items-start gap-2">
                          <Shield className="w-3.5 h-3.5 text-foreground/30 mt-0.5" />
                          <div className="min-w-0">
                            <p className="text-[10px] text-foreground/40">{t('imSettings.agentId')}</p>
                            <p className="text-xs text-foreground/60 font-mono truncate">{channel.config.agentId}</p>
                          </div>
                        </div>
                      )}
                      {channel.config.appId && (
                        <div className="flex items-start gap-2">
                          <Shield className="w-3.5 h-3.5 text-foreground/30 mt-0.5" />
                          <div className="min-w-0">
                            <p className="text-[10px] text-foreground/40">{t('imSettings.appId')}</p>
                            <p className="text-xs text-foreground/60 font-mono truncate">{channel.config.appId}</p>
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>
            )
          })
        )}
      </div>

      {showModal && (
        <>
          <div className="fixed inset-0 bg-black/50 z-30" onClick={() => setShowModal(false)} />
          <div className="fixed inset-0 z-40 flex items-center justify-center p-4">
            <div className="w-full max-w-md rounded-xl border border-border bg-card shadow-2xl">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground/90">
                  {editingChannel ? t('imSettings.editChannel') : t('imSettings.addChannel')}
                </span>
                <button onClick={() => setShowModal(false)} className="p-1 rounded hover:bg-foreground/10 transition-colors">
                  <X className="w-4 h-4 text-foreground/50" />
                </button>
              </div>

              <div className="px-4 py-4 space-y-3">
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.channelType')}</label>
                  <div className="relative">
                    <select
                      value={selectedChannelType}
                      onChange={e => setSelectedChannelType(e.target.value)}
                      disabled={!!editingChannel}
                      className="w-full appearance-none px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 outline-none focus:border-foreground/20 transition-colors cursor-pointer disabled:opacity-50"
                    >
                      {channelTypes.map((c) => (
                        <option key={c.id} value={c.id} className="bg-card text-foreground/80">
                          {c.icon} {c.name}
                        </option>
                      ))}
                    </select>
                    <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-foreground/40 pointer-events-none" />
                  </div>
                </div>

                {selectedChannelType === 'dingtalk' && (
                  <>
                    {/* Webhook 模式 */}
                    <div className="flex items-center gap-2 mb-2">
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/50 font-mono">Webhook</span>
                      <span className="text-[10px] text-foreground/30">简单发送消息</span>
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.webhook')}</label>
                      <input
                        value={form.webhook}
                        onChange={e => setForm(f => ({ ...f, webhook: e.target.value }))}
                        placeholder="https://oapi.dingtalk.com/robot/send?access_token=xxx"
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.secret')}</label>
                      <input
                        value={form.secret}
                        onChange={e => setForm(f => ({ ...f, secret: e.target.value }))}
                        placeholder={t('imSettings.secretPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>

                    {/* Stream 模式 */}
                    <div className="flex items-center gap-2 mb-2 mt-4 pt-3 border-t border-border/50">
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400 font-mono">Stream</span>
                      <span className="text-[10px] text-foreground/30">双向 WebSocket 长连接</span>
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">Client ID</label>
                      <input
                        value={form.clientId || ''}
                        onChange={e => setForm(f => ({ ...f, clientId: e.target.value }))}
                        placeholder="应用 Client ID（从钉钉开放平台获取）"
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">Client Secret</label>
                      <input
                        value={form.clientSecret || ''}
                        onChange={e => setForm(f => ({ ...f, clientSecret: e.target.value }))}
                        type="password"
                        placeholder="应用 Client Secret"
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                  </>
                )}

                {selectedChannelType === 'feishu' && (
                  <>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.appId')}</label>
                      <input
                        value={form.appId}
                        onChange={e => setForm(f => ({ ...f, appId: e.target.value }))}
                        placeholder={t('imSettings.appIdPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.appSecret')}</label>
                      <input
                        value={form.appSecret}
                        onChange={e => setForm(f => ({ ...f, appSecret: e.target.value }))}
                        type="password"
                        placeholder={t('imSettings.appSecretPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.agentId')}</label>
                      <input
                        value={form.agentId}
                        onChange={e => setForm(f => ({ ...f, agentId: e.target.value }))}
                        placeholder={t('imSettings.agentIdPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.corpId')}</label>
                      <input
                        value={form.corpId}
                        onChange={e => setForm(f => ({ ...f, corpId: e.target.value }))}
                        placeholder={t('imSettings.corpIdPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                  </>
                )}
              </div>

              {saveError && (
                <div className="px-4 pb-2">
                  <div className="px-3 py-1.5 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-400">
                    {saveError}
                  </div>
                </div>
              )}

              <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
                <button
                  onClick={() => setShowModal(false)}
                  className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
                >
                  {t('imSettings.cancel')}
                </button>
                <button
                  onClick={handleSave}
                  className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 text-xs text-white font-medium transition-colors"
                >
                  {t('imSettings.save')}
                </button>
              </div>
            </div>
          </div>
        </>
      )}

      {showDeleteConfirm && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="w-full max-w-sm rounded-xl border border-border bg-card shadow-2xl">
            <div className="px-4 py-4">
              <p className="text-sm text-foreground/90 font-medium">{t('imSettings.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('imSettings.deleteChannelWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button
                onClick={() => setShowDeleteConfirm(null)}
                className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
              >
                {t('imSettings.cancel')}
              </button>
              <button
                onClick={confirmDelete}
                className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors"
              >
                {t('imSettings.delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
