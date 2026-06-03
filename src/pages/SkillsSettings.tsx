import { useState, useEffect, useCallback, useRef } from 'react'
import { Trash2, Puzzle, ArrowLeft, Upload } from 'lucide-react'
import { useApi } from '@/hooks/useApi'
import { useTranslation } from 'react-i18next'
import type { Skill } from '@/types'

interface SkillsSettingsProps {
  onBack?: () => void
}

export function SkillsSettings({ onBack }: SkillsSettingsProps) {
  const { t } = useTranslation()
  const [skills, setSkills] = useState<Skill[]>([])
  const [uploading, setUploading] = useState(false)
  const [uploadError, setUploadError] = useState<string | null>(null)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const { listSkills, deleteSkill, uploadSkill, toggleSkill, loading } = useApi()

  const loadSkills = useCallback(() => {
    listSkills().then(setSkills).catch(() => {})
  }, [listSkills])

  const handleToggle = useCallback(async (id: string) => {
    try {
      const enabled = await toggleSkill(id)
      setSkills(prev => prev.map(s => s.id === id ? { ...s, enabled } : s))
    } catch {}
  }, [toggleSkill])

  useEffect(() => {
    loadSkills()
  }, [loadSkills])

  const handleFileSelect = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return

    // 重置 input，允许重复选择同一个文件
    e.target.value = ''

    setUploading(true)
    setUploadError(null)
    try {
      const result = await uploadSkill(file)
      loadSkills()
      if (result.errors && result.errors.length > 0) {
        setUploadError(t('settings.uploadSuccessPart', { installed: result.installed, errors: result.errors.length }))
      }
    } catch (err) {
      setUploadError(err instanceof Error ? err.message : t('settings.uploadError'))
    } finally {
      setUploading(false)
    }
  }, [uploadSkill, loadSkills])

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
          <span className="text-sm font-medium text-foreground/90">{t('settings.skillsTitle')}</span>
        </div>
        <button
          onClick={() => fileInputRef.current?.click()}
          disabled={uploading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:opacity-50 text-white text-xs font-medium transition-colors"
        >
          <Upload className="w-3.5 h-3.5" />
          {uploading ? t('settings.uploading') : t('settings.uploadSkill')}
        </button>
      </div>

      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".zip"
        className="hidden"
        onChange={handleFileSelect}
      />

      {/* Upload error */}
      {uploadError && (
        <div className="mx-4 mt-3 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20">
          <p className="text-xs text-red-400">{uploadError}</p>
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {loading && skills.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <p className="text-sm text-foreground/40">{t('common.loading')}</p>
          </div>
        ) : skills.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Puzzle className="w-10 h-10 text-foreground/20 mb-3" />
            <p className="text-sm text-foreground/40">{t('settings.noSkills')}</p>
            <p className="text-xs text-foreground/30 mt-2">
              {t('settings.agentLoadSkill')}
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
                    </div>
                    <p className="text-xs text-foreground/40 mt-0.5 truncate">{skill.description}</p>
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <button
                    onClick={() => handleToggle(skill.id)}
                    className={`relative w-7 h-3.5 rounded-full transition-colors mx-1 ${skill.enabled ? 'bg-green-500' : 'bg-foreground/20'}`}
                    title={skill.enabled ? '点击停用' : '点击启用'}
                  >
                    <div
                      className={`absolute top-0.5 w-2.5 h-2.5 rounded-full bg-foreground transition-transform ${skill.enabled ? 'translate-x-3.5' : 'translate-x-0.5'}`}
                    />
                  </button>
                  <button
                    onClick={() => setShowDeleteConfirm(skill.id)}
                    className="p-1 rounded hover:bg-red-500/10 transition-colors"
                    title={t('common.delete')}
                  >
                    <Trash2 className="w-3.5 h-3.5 text-foreground/40 hover:text-red-400" />
                  </button>
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Delete confirmation */}
      {showDeleteConfirm && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="w-full max-w-sm rounded-xl border border-border bg-card shadow-2xl">
            <div className="px-4 py-4">
              <p className="text-sm text-foreground/90 font-medium">{t('settings.confirmDelete')}</p>
              <p className="text-xs text-foreground/50 mt-2">{t('settings.deleteSkillWarning')}</p>
            </div>
            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
              <button onClick={() => setShowDeleteConfirm(null)}
                className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors">{t('common.cancel')}</button>
              <button onClick={confirmDelete}
                className="px-4 py-1.5 rounded-lg bg-red-500 hover:bg-red-400 text-xs text-white font-medium transition-colors">{t('common.delete')}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
