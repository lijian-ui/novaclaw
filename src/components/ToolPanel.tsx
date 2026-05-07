import { useState } from 'react'
import { Wrench, ChevronDown, ChevronRight, Play, AlertCircle } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'

interface ToolParam {
  name: string
  type_: string
  description: string
  required: boolean
}

interface ToolDefinition {
  name: string
  description: string
  parameters: ToolParam[]
  requires_confirmation: boolean
  danger_level: string
}

interface ToolPanelProps {
  tools: ToolDefinition[]
  onToolSelect?: (tool: ToolDefinition) => void
}

export function ToolPanel({ tools, onToolSelect }: ToolPanelProps) {
  const [expandedTools, setExpandedTools] = useState<Set<string>>(new Set())
  const [selectedToolId, setSelectedToolId] = useState<string | null>(null)

  const toggleExpand = (toolName: string) => {
    setExpandedTools(prev => {
      const next = new Set(prev)
      if (next.has(toolName)) next.delete(toolName)
      else next.add(toolName)
      return next
    })
  }

  if (tools.length === 0) {
    return (
      <Card>
        <CardContent className="p-6">
          <div className="flex flex-col items-center justify-center text-muted-foreground">
            <Wrench className="w-12 h-12 mb-3 opacity-50" />
            <p className="text-sm font-medium">暂无可用工具</p>
            <p className="text-xs mt-1">没有可用的工具定义</p>
          </div>
        </CardContent>
      </Card>
    )
  }

  return (
    <div className="space-y-2">
      {tools.map((tool) => {
        const isExpanded = expandedTools.has(tool.name)
        const isSelected = selectedToolId === tool.name

        return (
          <Card
            key={tool.name}
            className={`cursor-pointer transition-colors ${
              isSelected ? 'border-primary' : ''
            }`}
            onClick={() => {
              setSelectedToolId(tool.name)
              onToolSelect?.(tool)
            }}
          >
            <CardHeader className="p-3 flex flex-row items-center justify-between">
              <div className="flex items-center gap-2">
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    toggleExpand(tool.name)
                  }}
                  className="p-0.5 rounded hover:bg-accent transition-colors"
                >
                  {isExpanded ? (
                    <ChevronDown className="w-4 h-4" />
                  ) : (
                    <ChevronRight className="w-4 h-4" />
                  )}
                </button>
                <Wrench className="w-4 h-4 text-muted-foreground" />
                <div>
                  <CardTitle className="text-sm font-medium">{tool.name}</CardTitle>
                  <p className="text-xs text-muted-foreground mt-0.5">{tool.description}</p>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                  tool.danger_level === 'Safe' ? 'bg-green-500/10 text-green-500' :
                  tool.danger_level === 'Critical' ? 'bg-red-500/10 text-red-500' :
                  'bg-yellow-500/10 text-yellow-500'
                }`}>
                  {tool.danger_level}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="w-7 h-7"
                  onClick={(e) => {
                    e.stopPropagation()
                    onToolSelect?.(tool)
                  }}
                >
                  <Play className="w-3.5 h-3.5" />
                </Button>
              </div>
            </CardHeader>

            {isExpanded && (
              <CardContent className="px-3 pb-3 pt-0">
                <div className="pl-8 space-y-2">
                  {tool.parameters.length > 0 ? (
                    <>
                      <p className="text-xs text-muted-foreground font-medium">参数:</p>
                      {tool.parameters.map((param: ToolParam) => (
                        <div key={param.name} className="flex items-start gap-2">
                          <div className="flex-1">
                            <div className="flex items-center gap-1.5">
                              <code className="text-xs bg-muted px-1 rounded">{param.name}</code>
                              <span className="text-[10px] text-muted-foreground">{param.type_}</span>
                              {param.required && (
                                <AlertCircle className="w-3 h-3 text-destructive" />
                              )}
                            </div>
                            <p className="text-xs text-muted-foreground mt-0.5">{param.description}</p>
                          </div>
                        </div>
                      ))}
                    </>
                  ) : (
                    <p className="text-xs text-muted-foreground">无参数</p>
                  )}
                </div>
              </CardContent>
            )}
          </Card>
        )
      })}
    </div>
  )
}
