import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, X, Clock, ArrowLeft, Pencil, Play } from 'lucide-react'
import { useTranslation } from 'react-i18next'

interface CronJob {
  id: string
  name: string
  schedule: string
  enabled: boolean
  payload: string
  created_at: string
  updated_at: string
  last_run_at: string | null
  next_run_at: string | null
  status: string
  run_count: number
  last_error: string | null
}

const API = 'http://127.0.0.1:3000/api/cron-jobs'
const emptyForm = { name: '', cron: '', payload: '' }

interface ScheduledTasksPageProps {
  onBack?: () => void
}

export function ScheduledTasksPage({ onBack }: ScheduledTasksPageProps) {
  const { t } = useTranslation()
  const [tasks, setTasks] = useState<CronJob[]>([])
  const [showModal, setShowModal] = useState(false)
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const [editingTask, setEditingTask] = useState<CronJob | null>(null)
  const [loading, setLoading] = useState(false)

  const loadTasks = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetch(API)
      if (res.ok) {
        const body = await res.json()
        setTasks(Array.isArray(body) ? body : (body.data || []))
      }
    } catch { /* ignore */ }
    setLoading(false)
  }, [])

  useEffect(() => { loadTasks() }, [loadTasks])

  const handleToggle = useCallback(async (id: string) => {
    try {
      await fetch(`${API}/${id}/toggle`, { method: 'POST' })
      loadTasks()
    } catch {}
  }, [loadTasks])

  const handleSave = useCallback(async () => {
    if (!form.name.trim()) return
    try {
      if (editingTask) {
        await fetch(`${API}/${editingTask.id}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name: form.name.trim(), schedule: form.cron.trim() || '* * * * *', payload: form.payload.trim() }),
        })
      } else {
        await fetch(API, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name: form.name.trim(), schedule: form.cron.trim() || '* * * * *', payload: form.payload.trim() }),
        })
      }
      setShowModal(false)
      setEditingTask(null)
      setForm(emptyForm)
      loadTasks()
    } catch {}
  }, [form, editingTask, loadTasks])

  const confirmDelete = useCallback(async () => {
    if (!showDeleteConfirm) return
    try {
      await fetch(`${API}/${showDeleteConfirm}`, { method: 'DELETE' })
      setShowDeleteConfirm(null)
      loadTasks()
    } catch {}
  }, [showDeleteConfirm, loadTasks])

  const handleRun = useCallback(async (id: string) => {
    try {
      await fetch(`${API}/${id}/run`, { method: 'POST' })
      loadTasks()
    } catch {}
  }, [loadTasks])

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3.5 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">{t('scheduledTasksPage.title')}</span>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={() => { setEditingTask(null); setForm(emptyForm); setShowModal(true) }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 text-white text-xs font-medium transition-colors">
            <Plus className="w-3.5 h-3.5" />
            {t('scheduledTasksPage.add')}
          </button>
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {loading && tasks.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <p className="text-sm text-foreground/40">{t('scheduledTasksPage.loading')}</p>
          </div>
        ) : tasks.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Clock className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">{t('scheduledTasksPage.noTasks')}</p>
            <p className="text-xs text-foreground/30 mt-2">{t('scheduledTasksPage.noTasksHint')}</p>
          </div>
        ) : (
          tasks.map((task) => (
            <div key={task.id} className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors">
              <div className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-3 min-w-0">
                  <div className={`p-2 rounded-lg ${task.enabled ? 'bg-green-500/10' : 'bg-foreground/5'}`}>
                    <Clock className={`w-4 h-4 ${task.enabled ? 'text-green-400' : 'text-foreground/30'}`} />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground/90">{task.name}</span>
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/50 font-mono">{task.schedule}</span>
                    </div>
                    <p className="text-xs text-foreground/40 mt-0.5 truncate">{task.payload}</p>
                    <div className="flex items-center gap-2 mt-1">
                      <span className="text-[10px] text-foreground/30">{task.status} · 执行 {task.run_count} 次</span>
                      {task.last_error && <span className="text-[10px] text-red-400/60">{task.last_error}</span>}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <button onClick={() => { setEditingTask(task); setForm({ name: task.name, cron: task.schedule, payload: task.payload }); setShowModal(true) }}
                    className="p-1 rounded hover:bg-foreground/10 transition-colors" title="编辑">
                    <Pencil className="w-3.5 h-3.5 text-foreground/40 hover:text-blue-400" />
                  </button>
                  <button onClick={() => handleRun(task.id)}
                    className="p-1 rounded hover:bg-green-500/10 transition-colors" title="手动执行">
                    <Play className="w-3.5 h-3.5 text-foreground/40 hover:text-green-400" />
                  </button>
                  <button onClick={() => handleToggle(task.id)}
                    className={`relative w-8 h-4 rounded-full transition-colors mx-1 ${task.enabled ? 'bg-green-500' : 'bg-foreground/20'}`}
                    title={task.enabled ? t('scheduledTasksPage.disable') : t('scheduledTasksPage.enable')}>
                    <div className={`absolute top-0.5 w-3 h-3 rounded-full bg-foreground transition-transform ${task.enabled ? 'translate-x-4' : 'translate-x-0.5'}`} />
                  </button>
                  <button onClick={() => setShowDeleteConfirm(task.id)} className="p-1 rounded hover:bg-red-500/10 transition-colors" title={t('scheduledTasksPage.delete')}>
                    <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                  </button>
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Add/Edit Modal (Cron) */}
      {showModal && (
        <>
          <div className="fixed inset-0 bg-black/50 z-30" onClick={() => { setShowModal(false); setEditingTask(null); setForm(emptyForm) }} />
          <div className="fixed inset-0 z-40 flex items-center justify-center p-4">
            <div className="w-full max-w-md rounded-xl border border-border bg-card shadow-2xl">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground/90">
                  {editingTask ? '编辑定时任务' : t('scheduledTasksPage.addModalTitle')}
                </span>
                <button onClick={() => { setShowModal(false); setEditingTask(null); setForm(emptyForm) }} className="p-1 rounded hover:bg-foreground/10 transition-colors">
                  <X className="w-4 h-4 text-foreground/50" />
                </button>
              </div>
              <div className="px-4 py-4 space-y-3">
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('scheduledTasksPage.name')}</label>
                  <input value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder={t('scheduledTasksPage.placeholderName')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors" />
                </div>
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('scheduledTasksPage.cronExpression')}</label>
                  <input value={form.cron} onChange={e => setForm(f => ({ ...f, cron: e.target.value }))}
                    placeholder={t('scheduledTasksPage.placeholderCron')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors font-mono" />
                </div>
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">{t('scheduledTasksPage.payload')}</label>
                  <input value={form.payload} onChange={e => setForm(f => ({ ...f, payload: e.target.value }))}
                    placeholder={t('scheduledTasksPage.placeholderPayload')} className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors" />
                </div>
              </div>
              <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
                <button onClick={() => { setShowModal(false); setEditingTask(null); setForm(emptyForm) }} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('scheduledTasksPage.cancel')}</button>
                <button onClick={handleSave} disabled={!form.name.trim()} className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium transition-colors">
                  {editingTask ? '保存' : t('scheduledTasksPage.add')}
                </button>
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
              <p className="text-sm text-foreground/90 font-medium">{t('scheduledTasksPage.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('scheduledTasksPage.deleteWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button onClick={() => setShowDeleteConfirm(null)} className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('scheduledTasksPage.cancel')}</button>
              <button onClick={confirmDelete} className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors">{t('scheduledTasksPage.delete')}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
