import { useEffect, useRef } from 'react'
import { AlertTriangle, X } from 'lucide-react'

interface ConfirmDialogProps {
  open: boolean
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  variant?: 'danger' | 'default'
  onConfirm: () => void
  onCancel: () => void
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = '确认',
  cancelLabel = '取消',
  variant = 'default',
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const confirmRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    if (open) {
      // 延迟聚焦让动画先播放
      setTimeout(() => confirmRef.current?.focus(), 100)
    }
  }, [open])

  useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, onCancel])

  if (!open) return null

  return (
    <div
      className="fixed inset-0 z-[10000] flex items-center justify-center"
      onClick={onCancel}
    >
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" />

      {/* Dialog */}
      <div
        className="relative bg-mainbg border border-border rounded-xl shadow-2xl w-full max-w-sm mx-4 p-0 overflow-hidden animate-in fade-in zoom-in-95 duration-150"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start gap-3 px-5 pt-5 pb-3">
          <div className={`w-10 h-10 rounded-full flex items-center justify-center shrink-0 ${
            variant === 'danger'
              ? 'bg-red-500/15 text-red-400'
              : 'bg-blue-500/15 text-blue-400'
          }`}>
            <AlertTriangle className="w-5 h-5" />
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="text-sm font-medium text-foreground/80">{title}</h3>
            <p className="text-xs text-foreground/40 mt-1 leading-relaxed">{message}</p>
          </div>
          <button
            onClick={onCancel}
            className="p-1 rounded-md hover:bg-foreground/10 transition-colors shrink-0 mt-0.5"
          >
            <X className="w-4 h-4 text-foreground/40" />
          </button>
        </div>

        {/* Actions */}
        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border">
          <button
            onClick={onCancel}
            className="px-4 py-1.5 rounded-lg text-xs text-foreground/50 hover:bg-foreground/10 transition-colors"
          >
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            onClick={onConfirm}
            className={`px-4 py-1.5 rounded-lg text-xs text-white font-medium transition-colors ${
              variant === 'danger'
                ? 'bg-red-500 hover:bg-red-400'
                : 'bg-blue-500 hover:bg-blue-400'
            }`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
