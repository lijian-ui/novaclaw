import { useState } from 'react'

interface ContextRingProps {
  used: number
  total: number
}

export function ContextRing({ used, total }: ContextRingProps) {
  const [hovered, setHovered] = useState(false)

  const ratio = total > 0 ? Math.min(used / total, 1) : 0
  const percentage = ratio * 100
  const size = 28
  const center = size / 2
  const radius = 11
  const strokeWidth = 2.5
  const circumference = 2 * Math.PI * radius
  const dashOffset = circumference * (1 - ratio)
  const strokeColor = ratio > 0.9 ? '#ef4444' : ratio > 0.7 ? '#f59e0b' : '#10b981'

  const formatTokens = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
    return `${n}`
  }

  if (total <= 0) return null

  return (
    <div
      className="relative flex items-center justify-center"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="transform -rotate-90">
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeWidth={strokeWidth}
          className="text-foreground/10"
        />
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          stroke={strokeColor}
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
          className="transition-all duration-300"
        />
      </svg>
      <span className="absolute text-[8px] font-semibold leading-none pointer-events-none">
        {ratio >= 0.01 ? `${(percentage).toFixed(0)}%` : '0%'}
      </span>

      {hovered && (
        <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2.5 py-1.5 rounded-md bg-popover border border-border shadow-lg text-xs whitespace-nowrap z-30 pointer-events-none">
          <div className="text-foreground/80">
            <span className="font-medium">{percentage.toFixed(1)}%</span>
            <span className="text-foreground/50"> 上下文已使用</span>
          </div>
          <div className="text-foreground/60">
            {formatTokens(used)} / {formatTokens(total)}
          </div>
        </div>
      )}
    </div>
  )
}