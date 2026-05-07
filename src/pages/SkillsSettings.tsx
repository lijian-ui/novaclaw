import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, X, Puzzle, ArrowLeft } from 'lucide-react'
import { useApi } from '@/hooks/useApi'
import type { Skill } from '@/types'

const emptyForm = { name: '', description: '', author: '' }

interface SkillsSettingsProps {
  onBack?: () => void
}

export function SkillsSettings({ onBack }: SkillsSettingsProps) {
  const [skills, setSkills] = useState<Skill[]>([])
  const [showModal, setShowModal] = useState(false)
  const [form, setForm] = useState(emptyForm)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const { listSkills, deleteSkill, loading } = useApi()

  const loadSkills = useCallback(() => {
    listSkills().then(setSkills).catch(() => {})
  }, [listSkills])

  useEffect(() => {
    loadSkills()
  }, [loadSkills])

  const handleToggle = useCallback(async (id: string) => {
    try {
      const response = await fetch(`/api/skills/${id}/toggle`, { method: 'POST' })
      if (response.ok) {
        loadSkills()
      }
    } catch {}
  }, [loadSkills])

  const handleSave = useCallback(async () => {
    if (!form.name.trim()) return
    try {
      const response = await fetch('/api/skills', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: form.name.trim(),
          description: form.description.trim(),
          author: form.author.trim() || undefined,
        }),
      })
      if (response.ok) {
        setShowModal(false)
        setForm(emptyForm)
        loadSkills()
      }
    } catch {}
  }, [form, loadSkills])

  const confirmDelete = useCallback(async () => {
    if (!showDeleteConfirm) return
    try {
      await deleteSkill(showDeleteConfirm)
      setShowDeleteConfirm(null)
      loadSkills()
    } catch {}
  }, [showDeleteConfirm, deleteSkill, loadSkills])

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <ArrowLeft className="w-4 h-4 text-foreground/60" />
          </button>
          <span className="text-sm font-medium text-foreground/90">技能配置</span>
        </div>
        <button
          onClick={() => { setForm(emptyForm); setShowModal(true) }}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 text-xs transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          添加技能
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {loading && skills.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <p className="text-sm text-foreground/40">加载中...</p>
          </div>
        ) : skills.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Puzzle className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">暂无技能，点击右上角添加</p>
            <p className="text-xs text-foreground/30 mt-2">
              Agent 也可以通过 `skill_manager` 工具自主创建技能
            </p>
          </div>
        ) : (
          skills.map((skill) => (
            <div
              key={skill.id}
              className="rounded-xl border border-border bg-foreground/[0.02] hover:bg-foreground/[0.04] transition-colors"
            >
              <div className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-3 min-w-0">
                  <div className={`p-2 rounded-lg ${skill.enabled ? 'bg-green-500/10' : 'bg-foreground/5'}`}>
                    <Puzzle className={`w-4 h-4 ${skill.enabled ? 'text-green-400' : 'text-foreground/30'}`} />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground/90">{skill.name}</span>
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/10 text-foreground/50">
                        v{skill.version}
                      </span>
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-foreground/5 text-foreground/40">
                        active
                      </span>
                    </div>
                    <p className="text-xs text-foreground/40 mt-0.5 truncate">{skill.description}</p>
                    <div className="flex items-center gap-2 mt-1">
                      <span className="text-[10px] text-foreground/30">
                        {skill.level} · 使用 {skill.use_count || 0} 次
                      </span>
                      {skill.tags?.length && skill.tags.length > 0 && (
                        <span className="text-[10px] text-foreground/30">
                          {skill.tags.slice(0, 3).join(', ')}
                        </span>
                      )}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <button
                    onClick={() => handleToggle(skill.id)}
                    className={`relative w-8 h-4 rounded-full transition-colors mx-1 ${
                      skill.enabled ? 'bg-green-500' : 'bg-foreground/20'
                    }`}
                    title={skill.enabled ? '停用' : '启用'}
                  >
                    <div
                      className={`absolute top-0.5 w-3 h-3 rounded-full bg-foreground transition-transform ${
                        skill.enabled ? 'translate-x-4' : 'translate-x-0.5'
                      }`}
                    />
                  </button>
                  <button
                    onClick={() => setShowDeleteConfirm(skill.id)}
                    className="p-1 rounded hover:bg-red-500/10 transition-colors"
                    title="删除"
                  >
                    <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                  </button>
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Add Modal */}
      {showModal && (
        <>
          <div className="fixed inset-0 bg-black/50 z-30" onClick={() => setShowModal(false)} />
          <div className="fixed inset-0 z-40 flex items-center justify-center p-4">
            <div className="w-full max-w-md rounded-xl border border-border bg-card shadow-2xl">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <span className="text-sm font-medium text-foreground/90">添加技能</span>
                <button onClick={() => setShowModal(false)} className="p-1 rounded hover:bg-foreground/10 transition-colors">
                  <X className="w-4 h-4 text-foreground/50" />
                </button>
              </div>
              <div className="px-4 py-4 space-y-3">
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">技能名称 *</label>
                  <input value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder="例如: 代码审查"
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors" />
                </div>
                <div>
                  <label className="text-xs text-foreground/50 mb-1 block">描述</label>
                  <textarea value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
                    placeholder="技能描述"
                    rows={3}
                    className="w-full px-3 py-2 rounded-lg bg-foreground/5 border border-border text-sm text-foreground/80 placeholder-foreground/30 outline-none focus:border-foreground/20 transition-colors resize-none" />
                </div>
              </div>
              <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
                <button onClick={() => setShowModal(false)}
                  className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">取消</button>
                <button onClick={handleSave} disabled={!form.name.trim()}
                  className="px-4 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-xs text-white font-medium transition-colors">添加</button>
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
              <p className="text-xs text-foreground/50 mt-2">确定要删除此技能吗？此操作不可撤销。</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button onClick={() => setShowDeleteConfirm(null)}
                className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">取消</button>
              <button onClick={confirmDelete}
                className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors">删除</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
