import { useState, useCallback } from 'react'
import { Shield, X, Check, Ban, Plus, Loader2 } from 'lucide-react'
import { getApiBase } from '@/hooks/useApi'

interface ApprovalRequest {
  id: string
  command: string
  description: string
  toolName: string
}

interface ApprovalDialogProps {
  pending: ApprovalRequest | null
  onClose: () => void
}

export function ApprovalDialog({ pending, onClose }: ApprovalDialogProps) {
  const [sending, setSending] = useState(false)

  const callApprove = useCallback(async (decision: string) => {
    if (!pending) return
    setSending(true)
    try {
      if (decision === 'always_allow') {
        const prefix = pending.command.split(/\s+/)[0] || pending.command
        await fetch(`${getApiBase()}/config/shell_allowlist`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ prefix }),
        })
      }
      await fetch(`${getApiBase()}/chat/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ approval_id: pending.id, decision }),
      })
    } catch (e) {
      console.error('[Approval] 发送审批失败', e)
    }
    setSending(false)
    onClose()
  }, [pending, onClose])

  if (!pending) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div className="w-full max-w-lg mx-4 rounded-xl border border-border bg-mainbg shadow-2xl overflow-hidden">
        {/* 头部 */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Shield className="w-4 h-4 text-amber-400" />
            <span className="text-sm font-medium text-foreground/90">确认执行命令</span>
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-foreground/10 transition-colors">
            <X className="w-4 h-4 text-foreground/40" />
          </button>
        </div>

        {/* 命令详情 */}
        <div className="px-5 py-4 space-y-3">
          <div>
            <span className="text-[10px] text-foreground/40 uppercase tracking-wider">工具</span>
            <p className="text-xs text-foreground/60 font-mono mt-0.5">{pending.toolName}</p>
          </div>
          <div>
            <span className="text-[10px] text-foreground/40 uppercase tracking-wider">命令</span>
            <pre className="mt-0.5 p-2.5 rounded-lg bg-foreground/5 border border-border text-xs text-foreground/80 font-mono whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
              {pending.command}
            </pre>
          </div>
          {pending.description && pending.description !== '无' && (
            <div>
              <span className="text-[10px] text-foreground/40 uppercase tracking-wider">说明</span>
              <p className="mt-0.5 text-xs text-foreground/60">{pending.description}</p>
            </div>
          )}
        </div>

        {/* 操作按钮 */}
        <div className="flex items-center gap-2 px-5 py-3 border-t border-border bg-foreground/[0.02]">
          <button
            onClick={() => callApprove('deny')}
            disabled={sending}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-red-500 hover:bg-red-500/10 disabled:opacity-50 transition-colors"
          >
            <Ban className="w-3.5 h-3.5" />
            拒绝
          </button>
          <div className="flex-1" />
          <button
            onClick={() => callApprove('allow_once')}
            disabled={sending}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-green-500 hover:bg-green-500/10 border border-green-500/20 disabled:opacity-50 transition-colors"
          >
            {sending ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Check className="w-3.5 h-3.5" />}
            允许一次
          </button>
          <button
            onClick={() => callApprove('always_allow')}
            disabled={sending}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-white bg-blue-500 hover:bg-blue-400 disabled:opacity-50 transition-colors"
          >
            {sending ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Plus className="w-3.5 h-3.5" />}
            添加白名单
          </button>
        </div>
      </div>
    </div>
  )
}
