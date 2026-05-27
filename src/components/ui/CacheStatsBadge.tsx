import { useState } from 'react'

interface CacheStatsBadgeProps {
  hitRate: number
  hitTokens: number
  inputTokens: number
}

function formatTokens(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return `${n}`
}

function estimateCostSaved(hitTokens: number): string {
  const hitCached = hitTokens * (0.0002 - 0.0001) / 1_000
  if (hitCached < 0.001) return '<$0.001'
  return `$${hitCached.toFixed(4)}`
}

export function CacheStatsBadge({ hitRate, hitTokens, inputTokens }: CacheStatsBadgeProps) {
  const [hovered, setHovered] = useState(false)

  if (inputTokens <= 0 && hitTokens <= 0) return null

  const percentage = hitRate * 100
  const missedTokens = inputTokens > hitTokens ? inputTokens - hitTokens : 0

  return (
    <div
      className="relative flex items-center cursor-help"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <div className={`flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium border transition-colors ${
        percentage >= 80
          ? 'text-green-500 border-green-500/30 bg-green-500/10'
          : percentage >= 50
          ? 'text-yellow-500 border-yellow-500/30 bg-yellow-500/10'
          : 'text-foreground/50 border-border/50 bg-transparent'
      }`}>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
          <path d="M12 2a10 10 0 1 0 10 10 10 10 0 0 0-10-10z" />
          <path d="m9 12 2 2 4-4" />
        </svg>
        <span>{percentage.toFixed(0)}%</span>
      </div>

      {hovered && (
        <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2.5 py-1.5 rounded-md bg-popover border border-border shadow-lg text-xs whitespace-nowrap z-30 pointer-events-none">
          <div className="text-foreground/80 font-medium mb-1">缓存命中统计</div>
          <div className="text-foreground/70 space-y-0.5">
            <div className="flex justify-between gap-4">
              <span className="text-foreground/50">命中率</span>
              <span className={percentage >= 80 ? 'text-green-400' : percentage >= 50 ? 'text-yellow-400' : ''}>
                {percentage.toFixed(1)}%
              </span>
            </div>
            <div className="flex justify-between gap-4">
              <span className="text-foreground/50">缓存命中</span>
              <span>{formatTokens(hitTokens)}</span>
            </div>
            <div className="flex justify-between gap-4">
              <span className="text-foreground/50">缓存未命中</span>
              <span>{formatTokens(missedTokens)}</span>
            </div>
            <div className="border-t border-border/50 my-1" />
            <div className="flex justify-between gap-4 text-[11px]">
              <span className="text-foreground/50">节省成本（估算）</span>
              <span className="text-green-400 font-medium">{estimateCostSaved(hitTokens)}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}