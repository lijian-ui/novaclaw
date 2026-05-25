import { useMemo } from 'react'
import {
  MessageSquare,
  Puzzle,
  Brain,
  Cpu,
  Blocks,
  Settings,
  Terminal,
  Clock,
  FileText,
} from 'lucide-react'
import { MCPSettings } from './MCPSettings'
import { ModelSettings } from './ModelSettings'
import { SkillsSettings } from './SkillsSettings'
import { AgentSettings } from './AgentSettings'
import { IMSettings } from './IMSettings'
import { SettingsPage } from './SettingsPage'
import { ScheduledTasksPage } from './ScheduledTasksPage'
import { LogsPage } from './LogsPage'
import { TerminalPanel } from '@/components/TerminalPanel'
import { useTranslation } from 'react-i18next'

interface Tool {
  id: string
  nameKey: string
  icon: React.ElementType
  iconColor: string
}

const toolDefs: Tool[] = [
  { id: 'im', nameKey: 'dashboard.im', icon: MessageSquare, iconColor: 'text-emerald-400' },
  { id: 'skills', nameKey: 'dashboard.skills', icon: Puzzle, iconColor: 'text-violet-400' },
  { id: 'model', nameKey: 'dashboard.model', icon: Cpu, iconColor: 'text-blue-400' },
  { id: 'agent', nameKey: 'dashboard.agent', icon: Brain, iconColor: 'text-amber-400' },
  { id: 'mcp', nameKey: 'dashboard.mcp', icon: Blocks, iconColor: 'text-cyan-400' },
  { id: 'terminal', nameKey: 'dashboard.terminal', icon: Terminal, iconColor: 'text-green-400' },
  { id: 'schedule', nameKey: 'dashboard.schedule', icon: Clock, iconColor: 'text-orange-400' },
  { id: 'logs', nameKey: 'dashboard.logs', icon: FileText, iconColor: 'text-foreground/50' },
  { id: 'settings', nameKey: 'dashboard.settings', icon: Settings, iconColor: 'text-foreground/50' },
]

interface DashboardProps {
  activeTool?: string | null
  onOpenTool?: (tool: string | null) => void
  onToggleFilePanel?: () => void
  terminalOpen?: boolean
  onToggleTerminal?: () => void
}

export function Dashboard({ activeTool, onOpenTool, terminalOpen, onToggleTerminal }: DashboardProps) {
  const { t } = useTranslation()
  
  const tools = useMemo(() => toolDefs.map(tool => ({
    ...tool,
    name: t(tool.nameKey)
  })), [t])

  if (activeTool === 'mcp') {
    return <MCPSettings onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'model') {
    return <ModelSettings onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'skills') {
    return <SkillsSettings onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'agent') {
    return <AgentSettings onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'im') {
    return <IMSettings onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'settings') {
    return <SettingsPage onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'schedule') {
    return <ScheduledTasksPage onBack={() => onOpenTool?.(null)} />
  }
  if (activeTool === 'logs') {
    return <LogsPage onBack={() => onOpenTool?.(null)} />
  }

  return (
    <div className="h-full flex flex-col bg-mainbg">
      {/* Header - 与 ChatPanel header 保持相同高度（右侧等距占位） */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0 min-h-[52px]">
        <span className="text-sm font-medium text-foreground/90">{t('dashboard.title')}</span>
        <div className="flex items-center gap-1 opacity-0 pointer-events-none select-none">
          <div className="w-7 h-7" />
          <div className="w-7 h-7" />
          <div className="w-7 h-7" />
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 flex flex-col items-center justify-center overflow-y-auto">
        <h1 className="text-xl font-bold text-foreground mb-1">{t('dashboard.openTool')}</h1>
        <p className="text-sm text-foreground/50 mb-10">{t('dashboard.subtitle')}</p>

        <div className="grid grid-cols-3 gap-4 w-full max-w-[320px]">
          {tools.map((tool) => {
            const Icon = tool.icon
            return (
              <button
                key={tool.id}
                onClick={() => {
                  if (tool.id === 'terminal') {
                    onToggleTerminal?.()
                  } else {
                    onOpenTool?.(tool.id)
                  }
                }}
                className="flex flex-col items-center justify-center gap-3 aspect-square rounded-xl bg-foreground/[0.04] hover:bg-foreground/[0.08] transition-colors"
              >
                <Icon className={`w-6 h-6 ${tool.iconColor}`} />
                <span className="text-sm text-foreground/60">{tool.name}</span>
              </button>
            )
          })}
        </div>
      </div>

      {/* Terminal */}
      <TerminalPanel visible={terminalOpen ?? false} onClose={() => onToggleTerminal?.()} />
    </div>
  )
}
