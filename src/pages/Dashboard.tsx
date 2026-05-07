import { useState } from 'react'
import {
  Code2,
  Puzzle,
  Brain,
  Cpu,
  Blocks,
  Settings,
  Terminal,
  PanelRightClose,
  Sun,
  Moon,
  Clock,
  FileText,
} from 'lucide-react'
import { MCPSettings } from './MCPSettings'
import { ModelSettings } from './ModelSettings'
import { SkillsSettings } from './SkillsSettings'
import { AgentSettings } from './AgentSettings'
import { SettingsPage } from './SettingsPage'
import { ScheduledTasksPage } from './ScheduledTasksPage'
import { LogsPage } from './LogsPage'
import { TerminalPanel } from '@/components/TerminalPanel'
import { useTheme } from '@/contexts/ThemeContext'

interface Tool {
  id: string
  name: string
  icon: React.ElementType
  iconColor: string
}

const tools: Tool[] = [
  { id: 'editor', name: '编辑器', icon: Code2, iconColor: 'text-emerald-400' },
  { id: 'skills', name: '技能', icon: Puzzle, iconColor: 'text-violet-400' },
  { id: 'model', name: '模型', icon: Cpu, iconColor: 'text-blue-400' },
  { id: 'agent', name: '智能体', icon: Brain, iconColor: 'text-amber-400' },
  { id: 'mcp', name: 'MCP', icon: Blocks, iconColor: 'text-cyan-400' },
  { id: 'terminal', name: '终端', icon: Terminal, iconColor: 'text-green-400' },
  { id: 'schedule', name: '定时任务', icon: Clock, iconColor: 'text-orange-400' },
  { id: 'logs', name: '日志', icon: FileText, iconColor: 'text-foreground/50' },
  { id: 'settings', name: '设置', icon: Settings, iconColor: 'text-foreground/50' },
]

interface DashboardProps {
  activeTool?: string | null
  onOpenTool?: (tool: string | null) => void
  onToggleFilePanel?: () => void
}

export function Dashboard({ activeTool, onOpenTool, onToggleFilePanel }: DashboardProps) {
  const { theme, toggle } = useTheme()
  const [terminalOpen, setTerminalOpen] = useState(false)

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
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
        <span className="text-sm font-medium text-foreground/90">主控台</span>
        <div className="flex items-center gap-1">
          <button onClick={toggle} className="p-1.5 rounded hover:bg-foreground/10 transition-colors" title={theme === 'dark' ? '切换亮色主题' : '切换暗色主题'}>
            {theme === 'dark' ? <Sun className="w-4 h-4 text-foreground/60" /> : <Moon className="w-4 h-4 text-foreground/60" />}
          </button>
          <button
            onClick={() => setTerminalOpen(!terminalOpen)}
            className={`p-1.5 rounded hover:bg-foreground/10 transition-colors ${
              terminalOpen ? 'bg-foreground/10' : ''
            }`}
            title="终端"
          >
            <Terminal className="w-4 h-4 text-foreground/60" />
          </button>
          <button
            onClick={onToggleFilePanel}
            className="p-1.5 rounded hover:bg-foreground/10 transition-colors"
            title="打开/关闭文件预览"
          >
            <PanelRightClose className="w-4 h-4 text-foreground/60" />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 flex flex-col items-center justify-center overflow-y-auto">
        <h1 className="text-xl font-bold text-foreground mb-1">打开工具</h1>
        <p className="text-sm text-foreground/50 mb-10">使用工具，扩展更多能力</p>

        <div className="grid grid-cols-3 gap-3">
          {tools.map((tool) => {
            const Icon = tool.icon
            return (
              <button
                key={tool.id}
                onClick={() => {
                  if (tool.id === 'terminal') {
                    setTerminalOpen(!terminalOpen)
                  } else {
                    onOpenTool?.(tool.id)
                  }
                }}
                className="flex flex-col items-center justify-center gap-2 w-28 h-24 rounded-xl bg-foreground/[0.04] hover:bg-foreground/[0.08] transition-colors"
              >
                <Icon className={`w-5 h-5 ${tool.iconColor}`} />
                <span className="text-xs text-foreground/60">{tool.name}</span>
              </button>
            )
          })}
        </div>
      </div>

      {/* Terminal */}
      <TerminalPanel visible={terminalOpen} onClose={() => setTerminalOpen(false)} />
    </div>
  )
}
