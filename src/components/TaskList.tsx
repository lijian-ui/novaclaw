import { useState } from 'react'
import { ListTodo, CheckCircle2, Circle, ChevronRight, X } from 'lucide-react'

export interface TaskProgressItem {
  task_id: string
  description: string
  status: string
  priority: number
  result?: string
  quality_score?: number
  attempts: number
}

export interface TaskProgress {
  plan_id: string
  plan_name: string
  completed_count: number
  total_count: number
  progress: number
  status: string
  tasks: TaskProgressItem[]
}

interface TaskListProps {
  taskProgress: TaskProgress | null
  onClose?: () => void
  onTaskClick?: (task: TaskProgressItem) => void
}

export function TaskList({ taskProgress, onClose, onTaskClick }: TaskListProps) {
  const [expandedTask, setExpandedTask] = useState<string | null>(null)

  if (!taskProgress) {
    return null
  }

  const completedCount = taskProgress.completed_count
  const totalCount = taskProgress.total_count
  const progressPercent = Math.round(taskProgress.progress * 100)

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'completed':
        return <CheckCircle2 className="w-4 h-4 text-green-400" />
      case 'in_progress':
        return <Circle className="w-4 h-4 text-blue-400 fill-blue-400" />
      case 'failed':
        return <Circle className="w-4 h-4 text-red-400" />
      default:
        return <Circle className="w-4 h-4 text-foreground/30" />
    }
  }

  const getStatusText = (status: string) => {
    switch (status) {
      case 'completed':
        return '已完成'
      case 'in_progress':
        return '进行中'
      case 'failed':
        return '失败'
      default:
        return '待执行'
    }
  }

  return (
    <div className="mx-3 mb-3 rounded-lg bg-card border border-border overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 bg-foreground/5">
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5">
            <ListTodo className="w-4 h-4 text-green-400" />
            <span className="text-sm font-medium text-foreground/90">任务清单</span>
          </div>
          <span className="text-xs text-foreground/50">
            {completedCount}/{totalCount} 已完成
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-20 h-1.5 bg-foreground/10 rounded-full overflow-hidden">
            <div
              className="h-full bg-green-400 rounded-full transition-all duration-300"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
          {onClose && (
            <button
              onClick={onClose}
              className="p-1 rounded hover:bg-foreground/10 transition-colors"
            >
              <X className="w-3.5 h-3.5 text-foreground/50" />
            </button>
          )}
        </div>
      </div>

      {/* Task Items */}
      <div className="divide-y divide-border">
        {taskProgress.tasks.map((task) => (
          <div
            key={task.task_id}
            className={`px-4 py-2.5 hover:bg-foreground/5 transition-colors cursor-pointer ${
              expandedTask === task.task_id ? 'bg-foreground/5' : ''
            }`}
            onClick={() => {
              setExpandedTask(expandedTask === task.task_id ? null : task.task_id)
              onTaskClick?.(task)
            }}
          >
            <div className="flex items-start gap-3">
              <div className="mt-0.5 shrink-0">
                {getStatusIcon(task.status)}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between">
                  <span className={`text-sm ${
                    task.status === 'completed' 
                      ? 'text-foreground/60 line-through' 
                      : 'text-foreground/90'
                  }`}>
                    {task.description}
                  </span>
                  <ChevronRight 
                    className={`w-3.5 h-3.5 text-foreground/30 shrink-0 transition-transform ${
                      expandedTask === task.task_id ? 'rotate-90' : ''
                    }`} 
                  />
                </div>
                
                {/* Expanded Details */}
                {expandedTask === task.task_id && (
                  <div className="mt-2 pt-2 border-t border-border/50">
                    <div className="flex items-center gap-4 text-xs text-foreground/50">
                      <span className="flex items-center gap-1">
                        <span className="px-1.5 py-0.5 rounded bg-foreground/10">
                          优先级 {task.priority}
                        </span>
                      </span>
                      <span>{getStatusText(task.status)}</span>
                      {task.attempts > 0 && (
                        <span>尝试 {task.attempts} 次</span>
                      )}
                      {task.quality_score !== undefined && (
                        <span>质量评分 {task.quality_score.toFixed(1)}</span>
                      )}
                    </div>
                    {task.result && (
                      <div className="mt-2 text-xs text-foreground/70 bg-foreground/5 rounded px-2 py-1.5 max-h-20 overflow-y-auto">
                        {task.result}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}