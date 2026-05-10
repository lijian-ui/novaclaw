import { useState, useEffect, useCallback } from 'react'
import { Plus, Pencil, Trash2, X, Brain, ArrowLeft, ChevronDown, Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

const API = 'http://127.0.0.1:3000/api/agents'

interface Agent {
  id: string
  name: string
  description: string
  bot: string
  enabled: boolean
  is_default: boolean
  memory_summary?: string
  user_summary?: string
}

const botOptions = [
  { id: 'deepseek-chat', name: 'DeepSeek Chat' },
  { id: 'gpt-4o', name: 'GPT-4o' },
  { id: 'gpt-4o-mini', name: 'GPT-4o Mini' },
  { id: 'qwen-max', name: 'Qwen Max' },
  { id: 'glm-4', name: 'GLM-4' },
  { id: 'Auto', name: '自动选择' },
]

const emptyForm = { name: '', description: '', bot: 'Auto' }

interface AgentSettingsProps {
  onBack?: () => void
}

export function AgentSettings({ onBack }: AgentSettingsProps) {
  const { t } = useTranslation()
  const [agents, setAgents] = useState<Agent[]>([])
  const [loading, setLoading] = useState(false)
  const [showModal, setShowModal] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)

  const loadAgents = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetch(API)
      if (res.ok) setAgents(await res.json())
    } catch {}
    setLoading(false)
  }, [])

  useEffect(() => { loadAgents() }, [loadAgents])

  const handleSave = useCallback(async () => {
    if (!form.name.trim()) return

    if (editingId) {
      // Update existing
      try {
        await fetch(`${API}/${editingId}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name: form.name.trim(), description: form.description.trim(), bot: form.bot }),
        })
        setShowModal(false)
        loadAgents()
      } catch {}
    } else {
      // Create new agent
      const newId = form.name.trim().toLowerCase().replace(/\s+/g, '-')
      try {
        // Create agent directory with profile
        const profile = {
          id: newId, name: form.name.trim(), description: form.description.trim(),
          bot: form.bot, enabled: false, is_default: false,
        }
        const res = await fetch(`${API}/${newId}`, {
          method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(profile),
        })
        if (res.ok) {
          setShowModal(false)
          setForm(emptyForm)
          loadAgents()
          // Reload to pick up the new agent
        } else {
          // Fallback: POST to create
          await fetch(`${API}/${newId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(profile) })
          loadAgents()
        }
      } catch {}
    }
  }, [form, editingId, loadAgents])

  const handleToggle = useCallback(async (id: string) => {
    try {
      await fetch(`${API}/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: true }), // toggle will get current state
      })
      loadAgents()
    } catch {}
  }, [loadAgents])

  const confirmDelete = useCallback(async () => {
    if (!showDeleteConfirm) return
    try {
      await fetch(`${API}/${showDeleteConfirm}`, { method: 'DELETE' })
      setShowDeleteConfirm(null)
      loadAgents()
    } catch {}
  }, [showDeleteConfirm, loadAgents])

  const openEditModal = (agent: Agent) => {
    setEditingId(agent.id)
    setForm({ name: agent.name, description: agent.description, bot: agent.bot })
    setShowModal(true)
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('agentSettings.title')}</span>
        </div>
        <button onClick={() => { setEditingId(null); setForm(emptyForm); setShowModal(true) }}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 text-xs transition-colors">
          <Plus className="w-3.5 h-3.5" />{t('agentSettings.addAgent')}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {loading && agents.length === 0 ? (
          <div className="flex items-center justify-center h-full"><Loader2 className="w-5 h-5 animate-spin text-foreground/30" /></div>
        ) : agents.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Brain className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">{t('agentSettings.noAgents')}</p>
          </div>
        ) : (
          agents.map((agent) => (
            <div key={agent.id} className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors">
              <div className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-3 min-w-0 flex-1">
                  <div className={`p-2 rounded-lg ${agent.enabled ? 'bg-green-500/10' : 'bg-foreground/5'}`}>
                    <Brain className={`w-4 h-4 ${agent.enabled ? 'text-green-400' : 'text-foreground/30'}`} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground/90">{agent.name}</span>
                      {agent.is_default && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400">{t('agentSettings.default')}</span>
                      )}
                      <span className="text-[10px] text-foreground/40 font-mono">{agent.bot}</span>
                    </div>
                    <p className="text-xs text-foreground/40 mt-0.5">{agent.description}</p>
                    {agent.memory_summary && (
                      <p className="text-[10px] text-foreground/30 mt-0.5 truncate">{t('agentSettings.memory', { summary: agent.memory_summary })}</p>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <button onClick={() => openEditModal(agent)} className="p-1 rounded hover:bg-foreground/10 transition-colors" title={t('agentSettings.edit')}>
                    <Pencil className="w-3.5 h-3.5 text-foreground/40" />
                  </button>
                  <button onClick={() => handleToggle(agent.id)}
                    className={`relative w-8 h-4 rounded-full transition-colors mx-1 ${agent.enabled ? 'bg-green-500' : 'bg-foreground/20'}`}
                    title={agent.enabled ? t('agentSettings.disable') : t('agentSettings.enable')}>
                    <div className={`absolute top-0.5 w-3 h-3 rounded-full bg-foreground transition-transform ${agent.enabled ? 'translate-x-4' : 'translate-x-0.5'}`} />
                  </button>
                  {!agent.is_default && (
                    <button onClick={() => setShowDeleteConfirm(agent.id)} className="p-1 rounded hover:bg-red-500/10 transition-colors" title={t('agentSettings.delete')}>
                      <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Add/Edit Modal */}
      {showModal && (
        <>
          <div className="fixed inset-0 bg-black/50 z-30" onClick={() => setShowModal(false)} />
          <div className="fixed inset-0 z-40 flex items-center justify-center p-4">
            <div className="w-full max-w-md rounded-xl border border-border bg-card shadow-2xl">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground/90">{editingId ? t('agentSettings.editAgent') : t('agentSettings.addAgent')}</span>
                <button onClick={() => setShowModal(false)} className="p-1 rounded hover:bg-foreground/10 transition-colors"><X className="w-4 h-4 text-foreground/50" /></button>
              </div>
              <div className="px-4 py-4 space-y-3">
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('agentSettings.name')}</label>
                  <input value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} placeholder={t('agentSettings.placeholderName')}
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors" />
                </div>
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('agentSettings.description')}</label>
                  <textarea value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
                    placeholder={t('agentSettings.placeholderDescription')} rows={3}
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors resize-none" />
                </div>
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('agentSettings.associatedBot')}</label>
                  <div className="relative">
                    <select value={form.bot} onChange={e => setForm(f => ({ ...f, bot: e.target.value }))}
                      className="w-full appearance-none px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 outline-none focus:border-foreground/20 transition-colors cursor-pointer">
                      {botOptions.map((b) => (
                        <option key={b.id} value={b.id} className="bg-card text-foreground/80">{b.name}</option>
                      ))}
                    </select>
                    <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-foreground/40 pointer-events-none" />
                  </div>
                </div>
              </div>
              <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
                <button onClick={() => setShowModal(false)} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('agentSettings.cancel')}</button>
                <button onClick={handleSave} disabled={!form.name.trim()}
                  className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium transition-colors">{editingId ? t('agentSettings.save') : t('agentSettings.add')}</button>
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
              <p className="text-sm text-foreground/90 font-medium">{t('agentSettings.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('agentSettings.deleteAgentWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button onClick={() => setShowDeleteConfirm(null)} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('agentSettings.cancel')}</button>
              <button onClick={confirmDelete} className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors">{t('agentSettings.delete')}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
