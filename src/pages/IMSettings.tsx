import { useState, useEffect, useCallback, useRef } from 'react'
import { Plus, Trash2, Pencil, ChevronRight, ChevronDown, X, ArrowLeft, Webhook, Scan } from 'lucide-react'
import { getApiBase } from '@/hooks/useApi'
import { useTranslation } from 'react-i18next'
import dingtalkIcon from '@/assets/dingtalk.png'
import weixinIcon from '@/assets/weixin.png'
// import feishuIcon from '@/assets/feishu.png' // 待后端对接
import QRCode from 'qrcode'

interface IMChannel {
  id: string
  name: string
  channel_type: string
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
  { id: 'dingtalk', name: '钉钉', icon: dingtalkIcon, color: 'text-blue-400' },
  // { id: 'feishu', name: '飞书', icon: feishuIcon, color: 'text-green-400' }, // 待后端对接
  { id: 'weixin', name: '个人微信', icon: weixinIcon, color: 'text-emerald-400' },
]

interface FormState {
  name: string
  clientId: string
  clientSecret: string
  appId: string
  appSecret: string
  agentId: string
  corpId: string
}

const emptyForm: FormState = {
  name: '',
  clientId: '',
  clientSecret: '',
  appId: '',
  appSecret: '',
  agentId: '',
  corpId: '',
}

// 从通道配置推断所属平台类型
// ─── 微信扫码绑定组件 ──────────────────────────────────────
function WeChatBind({ onToken }: { onToken: (token: string, id: string) => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [scanning, setScanning] = useState(false)
  const [qrUrl, setQrUrl] = useState('')
  const [_sessionId, setSessionId] = useState('')
  const [status, setStatus] = useState('')

  // QR码渲染
  useEffect(() => {
    if (qrUrl && canvasRef.current) {
      QRCode.toCanvas(canvasRef.current, qrUrl, { width: 192, margin: 2 }, (err: Error | null | undefined) => {
        if (err) console.error('QR码渲染失败:', err)
      })
    }
  }, [qrUrl])

  const startScan = useCallback(async () => {
    setScanning(true)
    setStatus('正在获取二维码...')
    try {
      const res = await fetch(`${getApiBase()}/weixin/qrcode`)
      const body = await res.json()
      if (body.success && body.data) {
        setQrUrl(body.data.qrcode_url)
        setSessionId(body.data.session)
        setStatus('wait')

        // 开始轮询状态
        pollStatus(body.data.session)
      } else {
        setStatus('获取二维码失败: ' + (body.message || '未知错误'))
      }
    } catch (e) {
      setStatus('请求失败: ' + (e instanceof Error ? e.message : String(e)))
    }
  }, [])

  const pollStatus = useCallback(async (session: string) => {
    while (true) {
      try {
        const res = await fetch(`${getApiBase()}/weixin/status?session=${session}`)
        const body = await res.json()
        if (!body.success) {
          setStatus('轮询失败')
          break
        }
        setStatus(body.status)
        if (body.status === 'confirmed') {
          onToken(body.bot_token || '', body.ilink_bot_id || '')
          setScanning(false)
          break
        }
        if (body.status === 'expired' || body.status === 'invalid') {
          setScanning(false)
          break
        }
        // wait / scaned 继续轮询
      } catch {
        setStatus('连接中断')
        break
      }
    }
  }, [onToken])

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 mb-2">
        <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 font-mono">iLink</span>
        <span className="text-[10px] text-foreground/30">微信官方长轮询协议，需扫码登录</span>
      </div>

      {!scanning ? (
        <button
          onClick={startScan}
          className="w-full flex items-center justify-center gap-2 py-3 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-white text-sm font-medium transition-colors"
        >
          <Scan className="w-4 h-4" />
          扫码绑定微信
        </button>
      ) : (
        <div className="rounded-lg bg-foreground/5 border border-border p-4">
          {qrUrl && (
            <div className="flex flex-col items-center gap-3">
              <canvas ref={canvasRef} className="w-48 h-48 rounded-lg border border-border" />
              <div className="text-center">
                <p className="text-xs text-foreground/70">请用微信扫描二维码</p>
                <p className={`text-xs mt-1 ${
                  status === 'confirmed' ? 'text-green-400' :
                  status === 'scaned' ? 'text-blue-400' :
                  status === 'expired' ? 'text-red-400' :
                  'text-foreground/40'
                }`}>
                  {status === 'wait' && '等待扫码...'}
                  {status === 'scaned' && '已扫码，请在手机上确认...'}
                  {status === 'confirmed' && '✓ 绑定成功'}
                  {status === 'expired' && '二维码已过期，请重新获取'}
                  {(status !== 'wait' && status !== 'scaned' && status !== 'confirmed' && status !== 'expired') && status}
                </p>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function detectChannelType(channel: IMChannel): string {
  if (channel.config.appId || channel.config.appSecret) return 'feishu'
  if (channel.channel_type === 'weixin') return 'weixin'
  return 'dingtalk'
}

export function IMSettings({ onBack }: IMSettingsProps) {
  const { t } = useTranslation()

  const [channels, setChannels] = useState<IMChannel[]>([])
  const [expandedTypes, setExpandedTypes] = useState<Set<string>>(new Set(['dingtalk', 'feishu']))
  const [showModal, setShowModal] = useState(false)
  const [editingChannel, setEditingChannel] = useState<string | null>(null)
  const [selectedChannelType, setSelectedChannelType] = useState('dingtalk')
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)

  const loadChannels = useCallback(async () => {
    try {
      const response = await fetch(`${getApiBase()}/config/im_channels`)
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
      const response = await fetch(`${getApiBase()}/config/im_channels`, {
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

  const openAddModal = () => {
    setEditingChannel(null)
    setSelectedChannelType('dingtalk')
    setForm(emptyForm)
    setShowModal(true)
  }

  const openEditModal = (channel: IMChannel) => {
    setEditingChannel(channel.id)
    setSelectedChannelType(detectChannelType(channel))
    setForm({ ...emptyForm, name: channel.name, ...channel.config })
    setShowModal(true)
  }

  const handleSave = async () => {
    const selectedType = channelTypes.find(c => c.id === selectedChannelType)
    if (!selectedType) return

    const newChannel: IMChannel = {
      id: editingChannel || `im_${Date.now()}`,
      name: form.name || `${selectedType.name} 机器人`,
      channel_type: selectedChannelType,
      enabled: true,
      config: form,
    }

    let updated: IMChannel[]
    if (editingChannel) {
      updated = channels.map(c => c.id === editingChannel ? newChannel : c)
    } else {
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

  // 获取渠道在列表中显示的标识名称：优先用用户填的机器人名称
  const getChannelDisplayName = (ch: IMChannel): string => {
    if (ch.name && !channelTypes.some(ct => ct.name === ch.name)) return ch.name
    if (ch.config.clientId) return ch.config.clientId.slice(0, 24)
    return `bot_${ch.id.slice(-6)}`
  }

  const providerColors: Record<string, string> = {
    dingtalk: 'text-blue-400',
    feishu: 'text-green-400',
  }

  // 按平台类型分组（只渲染有渠道的平台）
  const channelsByType = channelTypes
    .map(ct => ({
      ...ct,
      channels: channels.filter(c => detectChannelType(c) === ct.id),
    }))
    .filter(g => g.channels.length > 0)

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
          onClick={openAddModal}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 text-white text-xs font-medium transition-colors"
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
          channelsByType.map((group) => (
            <div key={group.id} className="rounded-xl border border-border overflow-hidden">
              {/* Provider Header */}
              <div
                className="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-foreground/[0.04] transition-colors"
                onClick={() => {
                  setExpandedTypes(prev => {
                    const next = new Set(prev)
                    if (next.has(group.id)) next.delete(group.id)
                    else next.add(group.id)
                    return next
                  })
                }}
              >
                <div className="flex items-center gap-3">
                  <button className="p-0.5 rounded hover:bg-foreground/10 transition-colors">
                    {expandedTypes.has(group.id) ? (
                      <ChevronDown className="w-4 h-4 text-foreground/40" />
                    ) : (
                      <ChevronRight className="w-4 h-4 text-foreground/40" />
                    )}
                  </button>
                  <img src={group.icon} alt={group.name} className="w-5 h-5" />
                  <span className={`text-sm font-semibold ${providerColors[group.id] || 'text-foreground/80'}`}>
                    {group.name}
                  </span>
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/50">
                    {group.channels.length} 个
                  </span>
                </div>
              </div>

              {/* Sub-items */}
              {expandedTypes.has(group.id) && group.channels.length > 0 && (
                <div className="border-t border-border/50">
                  {group.channels.map((ch) => (
                    <div
                      key={ch.id}
                      className="flex items-center justify-between px-4 py-2.5 pl-14 hover:bg-foreground/[0.03] transition-colors border-b border-border/30 last:border-b-0"
                    >
                      <div className="flex items-center gap-2 min-w-0 flex-1">
                        <span className="text-xs font-mono text-foreground/70 truncate">
                          {getChannelDisplayName(ch)}
                        </span>
                        {ch.enabled ? (
                          <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/10 text-green-400 shrink-0">
                            {t('imSettings.enabled')}
                          </span>
                        ) : (
                          <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/40 shrink-0">
                            {t('imSettings.disabled')}
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-1 shrink-0">
                        <button
                          onClick={(e) => { e.stopPropagation(); openEditModal(ch) }}
                          className="p-1 rounded hover:bg-foreground/10 transition-colors"
                          title={t('imSettings.edit')}
                        >
                          <Pencil className="w-3.5 h-3.5 text-foreground/40" />
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); toggleEnabled(ch.id) }}
                          className={`relative w-9 h-5 rounded-full transition-colors ${
                            ch.enabled ? 'bg-green-500' : 'bg-foreground/20'
                          }`}
                          title={ch.enabled ? t('imSettings.disable') : t('imSettings.enable')}
                        >
                          <div
                            className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                              ch.enabled ? 'translate-x-[18px]' : 'translate-x-0.5'
                            }`}
                          />
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); setShowDeleteConfirm(ch.id) }}
                          className="p-1 rounded hover:bg-red-500/10 transition-colors"
                          title={t('imSettings.delete')}
                        >
                          <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              {expandedTypes.has(group.id) && group.channels.length === 0 && (
                <div className="px-4 py-3 border-t border-border/50">
                  <p className="text-xs text-foreground/30 text-center">{t('imSettings.noChannels')}</p>
                </div>
              )}
            </div>
          ))
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
                  <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.botName')}</label>
                  <input
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder={t('imSettings.botNamePlaceholder')}
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors"
                  />
                </div>
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
                        <option key={c.id} value={c.id}>
                          {c.name}
                        </option>
                      ))}
                    </select>
                    <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-foreground/40 pointer-events-none" />
                  </div>
                </div>

                {selectedChannelType === 'dingtalk' && (
                  <>
                    <div className="flex items-center gap-2 mb-2">
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

                {selectedChannelType === 'weixin' && (
                  <WeChatBind
                    onToken={(token, id) => {
                      setForm(f => ({
                        ...f,
                        clientId: token,
                        name: id || '个人微信',
                      }))
                    }}
                  />
                )}

                {selectedChannelType === 'feishu' && (
                  <>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.appId')}</label>
                      <input
                        value={form.appId || ''}
                        onChange={e => setForm(f => ({ ...f, appId: e.target.value }))}
                        placeholder={t('imSettings.appIdPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.appSecret')}</label>
                      <input
                        value={form.appSecret || ''}
                        onChange={e => setForm(f => ({ ...f, appSecret: e.target.value }))}
                        type="password"
                        placeholder={t('imSettings.appSecretPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.agentId')}</label>
                      <input
                        value={form.agentId || ''}
                        onChange={e => setForm(f => ({ ...f, agentId: e.target.value }))}
                        placeholder={t('imSettings.agentIdPlaceholder')}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('imSettings.corpId')}</label>
                      <input
                        value={form.corpId || ''}
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
