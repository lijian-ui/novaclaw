import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, Plug, X, ChevronRight, ChevronDown, Wrench, ArrowLeft, RefreshCw, Power, PowerOff, Loader2, Pencil } from 'lucide-react'
import { useTranslation } from 'react-i18next'

const API = 'http://127.0.0.1:3000/api/mcp'

interface McpTool {
  name: string
  description: string
  input_schema: any
}

interface McpConnection {
  name: string
  command: string
  args: string[]
  url?: string
  header?: Record<string, string>
  description: string
  enabled: boolean
  tools: McpTool[]
  transport_type: string
  status: 'disconnected' | 'connecting' | 'connected' | 'failed'
}

interface MCPSettingsProps {
  onBack?: () => void
}

const statusConfig: Record<string, { color: string; bg: string; label: string; dot: string }> = {
  connected: { color: 'text-green-400', bg: 'bg-green-500/10', label: '已连接', dot: 'bg-green-400' },
  connecting: { color: 'text-yellow-400', bg: 'bg-yellow-500/10', label: '连接中', dot: 'bg-yellow-400' },
  disconnected: { color: 'text-foreground/30', bg: 'bg-foreground/5', label: '未连接', dot: 'bg-foreground/20' },
  failed: { color: 'text-red-400', bg: 'bg-red-500/10', label: '连接失败', dot: 'bg-red-400' },
}

export function MCPSettings({ onBack }: MCPSettingsProps) {
  const { t } = useTranslation()
  const [servers, setServers] = useState<McpConnection[]>([])
  const [loading, setLoading] = useState(false)
  const [showModal, setShowModal] = useState(false)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [form, setForm] = useState({ name: '', transportType: 'stdio', command: '', args: '', url: '', headers: '', description: '' })
  const [editingServer, setEditingServer] = useState<string | null>(null)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const [discovering, setDiscovering] = useState<string | null>(null)
  const [connectingName, setConnectingName] = useState<string | null>(null)

  const loadServers = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetch(API)
      if (res.ok) setServers(await res.json())
    } catch {}
    setLoading(false)
  }, [])

  useEffect(() => { loadServers() }, [loadServers])

  const handleToggle = useCallback(async (name: string) => {
    try {
      await fetch(`${API}/${encodeURIComponent(name)}/toggle`, { method: 'POST' })
      loadServers()
    } catch {}
  }, [loadServers])

  const handleConnect = useCallback(async (name: string) => {
    setConnectingName(name)
    try {
      await fetch(`${API}/${encodeURIComponent(name)}/connect`, { method: 'POST' })
      loadServers()
    } catch {}
    setConnectingName(null)
  }, [loadServers])

  const handleDisconnect = useCallback(async (name: string) => {
    try {
      await fetch(`${API}/${encodeURIComponent(name)}/disconnect`, { method: 'POST' })
      loadServers()
    } catch {}
  }, [loadServers])

  const handleDiscover = useCallback(async (name: string) => {
    setDiscovering(name)
    try {
      const res = await fetch(`${API}/${encodeURIComponent(name)}/discover`, { method: 'POST' })
      const data = await res.json()
      if (!data.success) {
        console.warn('Discover failed:', data.message)
      }
      loadServers()
    } catch {}
    setDiscovering(null)
  }, [loadServers])

  const handleSave = useCallback(async () => {
    if (!form.name.trim()) return
    const isStdio = form.transportType === 'stdio'
    if (isStdio && !form.command.trim()) return
    if (!isStdio && !form.url.trim()) return

    setShowModal(false)

    const headers: Record<string, string> = {}
    if (form.headers.trim()) {
      form.headers.trim().split('\n').forEach(line => {
        const idx = line.indexOf(':')
        if (idx > 0) {
          headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim()
        }
      })
    }

    const body = JSON.stringify({
      name: form.name.trim(),
      transport_type: form.transportType,
      command: isStdio ? form.command.trim() : undefined,
      args: isStdio && form.args.trim() ? form.args.trim().split(/\s+/) : undefined,
      url: !isStdio ? form.url.trim() : undefined,
      headers: !isStdio && Object.keys(headers).length > 0 ? headers : undefined,
      description: form.description.trim() || undefined,
    })

    try {
      if (editingServer) {
        await fetch(`${API}/${encodeURIComponent(editingServer)}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body })
      } else {
        await fetch(API, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body })
      }
      setForm({ name: '', transportType: 'stdio', command: '', args: '', url: '', headers: '', description: '' })
      setEditingServer(null)
      loadServers()
    } catch {}
  }, [form, editingServer, loadServers])

  const confirmDelete = useCallback(async () => {
    if (!showDeleteConfirm) return
    try {
      await fetch(`${API}/${encodeURIComponent(showDeleteConfirm)}`, { method: 'DELETE' })
      setShowDeleteConfirm(null)
      loadServers()
    } catch {}
  }, [showDeleteConfirm, loadServers])

  const toggleExpanded = (name: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }

  const renderStatusBadge = (server: McpConnection) => {
    const cfg = statusConfig[server.status] || statusConfig.disconnected
    return (
      <span className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-medium ${cfg.color} ${cfg.bg}`}>
        {server.status === 'connecting' ? (
          <Loader2 className="w-2.5 h-2.5 animate-spin" />
        ) : (
          <span className={`w-1.5 h-1.5 rounded-full ${cfg.dot}`} />
        )}
        {cfg.label}
      </span>
    )
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('mcpSettings.title')}</span>
        </div>
        <button onClick={() => { setEditingServer(null); setForm({ name: '', transportType: 'stdio', command: '', args: '', url: '', headers: '', description: '' }); setShowModal(true) }}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 text-xs transition-colors">
          <Plus className="w-3.5 h-3.5" />
          {t('mcpSettings.addMcp')}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {loading && servers.length === 0 ? (
          <div className="flex items-center justify-center h-full text-sm text-foreground/40">{t('mcpSettings.loading')}</div>
        ) : servers.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Plug className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">{t('mcpSettings.noMcpConnections')}</p>
            <p className="text-xs text-foreground/30 mt-2">{t('mcpSettings.mcpHelp')}</p>
          </div>
        ) : (
          servers.map((server) => (
            <div key={server.name} className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors">
              <div className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-2 flex-1 min-w-0">
                  <button onClick={() => toggleExpanded(server.name)} className="p-0.5 rounded hover:bg-foreground/10 transition-colors shrink-0">
                    {expanded.has(server.name) ? <ChevronDown className="w-4 h-4 text-foreground/40" /> : <ChevronRight className="w-4 h-4 text-foreground/40" />}
                  </button>
                  <div className={`p-1.5 rounded-lg ${server.enabled ? 'bg-green-500/10' : 'bg-foreground/5'}`}>
                    <Plug className={`w-4 h-4 ${server.enabled ? 'text-green-400' : 'text-foreground/30'}`} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground/90">{server.name}</span>
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/50">
                        {t('mcpSettings.toolsCount', { count: (server.tools || []).length })}
                      </span>
                      {renderStatusBadge(server)}
                    </div>
                    <p className="text-xs text-foreground/40 mt-0.5 font-mono truncate">
                      {server.transport_type === 'stdio'
                        ? `${server.command} ${server.args?.join(' ') || ''}`
                        : server.url}
                    </p>
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  {server.transport_type === 'stdio' && (
                    server.status === 'connected' ? (
                      <button onClick={() => handleDisconnect(server.name)}
                        className="p-1 rounded hover:bg-foreground/10 transition-colors" title="断开">
                        <PowerOff className="w-3.5 h-3.5 text-red-400/60" />
                      </button>
                    ) : (
                      <button onClick={() => handleConnect(server.name)} disabled={connectingName === server.name}
                        className="p-1 rounded hover:bg-foreground/10 transition-colors" title="连接">
                        {connectingName === server.name
                          ? <Loader2 className="w-3.5 h-3.5 text-foreground/40 animate-spin" />
                          : <Power className="w-3.5 h-3.5 text-green-400/60" />
                        }
                      </button>
                    )
                  )}
                  <button onClick={() => {
                    const hdrs = server.header ? Object.entries(server.header).map(([k, v]) => `${k}: ${v}`).join('\n') : ''
                    setEditingServer(server.name)
                    setForm({
                      name: server.name, transportType: server.transport_type,
                      command: server.command || '', args: server.args?.join(' ') || '',
                      url: server.url || '', headers: hdrs, description: server.description,
                    })
                    setShowModal(true)
                  }} className="p-1 rounded hover:bg-foreground/10 transition-colors" title={t('mcpSettings.edit')}>
                    <Pencil className="w-3.5 h-3.5 text-foreground/40" />
                  </button>
                  <button onClick={() => handleDiscover(server.name)} disabled={discovering === server.name}
                    className="p-1 rounded hover:bg-foreground/10 transition-colors" title={t('mcpSettings.discoverTools')}>
                    <RefreshCw className={`w-3.5 h-3.5 text-foreground/40 ${discovering === server.name ? 'animate-spin' : ''}`} />
                  </button>
                  <button onClick={() => handleToggle(server.name)}
                    className={`relative w-8 h-4 rounded-full transition-colors mx-1 ${server.enabled ? 'bg-green-500' : 'bg-foreground/20'}`}
                    title={server.enabled ? t('mcpSettings.disable') : t('mcpSettings.enable')}>
                    <div className={`absolute top-0.5 w-3 h-3 rounded-full bg-foreground transition-transform ${server.enabled ? 'translate-x-4' : 'translate-x-0.5'}`} />
                  </button>
                  <button onClick={() => setShowDeleteConfirm(server.name)} className="p-1 rounded hover:bg-red-500/10 transition-colors" title={t('mcpSettings.delete')}>
                    <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                  </button>
                </div>
              </div>

              {expanded.has(server.name) && (server.tools || []).length > 0 && (
                <div className="px-4 py-3 border-t border-border space-y-2">
                  {server.tools.map((tool, idx) => (
                    <div key={idx} className="flex items-start gap-2.5 px-3 py-2 rounded-lg bg-foreground/[0.03]">
                      <Wrench className="w-3.5 h-3.5 mt-0.5 text-foreground/40 shrink-0" />
                      <div>
                        <p className="text-xs font-mono text-cyan-400/90">{tool.name}</p>
                        <p className="text-[11px] text-foreground/50 mt-0.5">{tool.description}</p>
                      </div>
                    </div>
                  ))}
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
                  {editingServer ? t('mcpSettings.editMcpConnection') || '编辑 MCP 连接' : t('mcpSettings.addMcpConnection')}
                </span>
                <button onClick={() => setShowModal(false)} className="p-1 rounded hover:bg-foreground/10 transition-colors"><X className="w-4 h-4 text-foreground/50" /></button>
              </div>
              <div className="px-4 py-4 space-y-3">
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.name')}</label>
                  <div className="relative">
                    <input value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} disabled={!!editingServer} placeholder="my-server" className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono disabled:opacity-50" />
                    {!editingServer && (
                      <p className="text-[10px] text-amber-400/70 mt-1">请使用英文名称，避免 LLM 调用异常</p>
                    )}
                  </div>
                </div>
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.transportType')}</label>
                  <div className="grid grid-cols-3 gap-2">
                    {(['stdio', 'sse', 'streamable-http'] as const).map((t) => (
                      <button key={t} onClick={() => setForm(f => ({ ...f, transportType: t }))}
                        className={`flex items-center justify-center px-3 py-2 rounded-lg border text-xs transition-colors ${
                          form.transportType === t
                            ? 'border-green-500/50 bg-green-500/10 text-green-400'
                            : 'border-border bg-foreground/5 text-foreground/50 hover:bg-foreground/10'
                        }`}>
                        {t}
                      </button>
                    ))}
                  </div>
                </div>

                {form.transportType === 'stdio' ? (
                  <>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.startCommand')}</label>
                      <input value={form.command} onChange={e => setForm(f => ({ ...f, command: e.target.value }))} placeholder={t('mcpSettings.placeholderCommand')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono" />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.args')}</label>
                      <input value={form.args} onChange={e => setForm(f => ({ ...f, args: e.target.value }))} placeholder={t('mcpSettings.placeholderArgs')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono" />
                    </div>
                  </>
                ) : (
                  <>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.url')}</label>
                      <input value={form.url} onChange={e => setForm(f => ({ ...f, url: e.target.value }))} placeholder={t('mcpSettings.placeholderUrl')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono" />
                    </div>
                    <div>
                      <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.headers')}</label>
                      <textarea value={form.headers} onChange={e => setForm(f => ({ ...f, headers: e.target.value }))}
                        placeholder={t('mcpSettings.placeholderHeaders')} rows={3}
                        className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono resize-none" />
                    </div>
                  </>
                )}

                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('mcpSettings.mcpDescription')}</label>
                  <input value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))} placeholder={t('mcpSettings.placeholderDescription')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors" />
                </div>

                {form.transportType === 'stdio' && (
                  <div className="px-3 py-2 rounded-lg bg-blue-500/5 border border-blue-500/20 text-[11px] text-blue-400/70">
                    保存后将自动连接并发现工具，无需重启服务
                  </div>
                )}
              </div>
              <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
                <button onClick={() => setShowModal(false)} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('mcpSettings.cancel')}</button>
                <button onClick={handleSave}
                  disabled={!form.name.trim() || (form.transportType === 'stdio' && !form.command.trim()) || (form.transportType !== 'stdio' && !form.url.trim())}
                  className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium transition-colors">{t('mcpSettings.add')}</button>
              </div>
            </div>
          </div>
        </>
      )}

      {showDeleteConfirm && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="w-full max-w-sm rounded-xl border border-border bg-card shadow-2xl">
            <div className="px-4 py-4">
              <p className="text-sm text-foreground/90 font-medium">{t('mcpSettings.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('mcpSettings.deleteMcpWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button onClick={() => setShowDeleteConfirm(null)} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('mcpSettings.cancel')}</button>
              <button onClick={confirmDelete} className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors">{t('mcpSettings.delete')}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
