import { useState, useEffect } from 'react'
import { Card, CardContent } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { useApi } from '@/hooks/useApi'
import type { Skill } from '@/types'
import { Wrench, Trash2, ToggleLeft, ToggleRight } from 'lucide-react'

export function Skills() {
  const [skills, setSkills] = useState<Skill[]>([])
  const { listSkills, deleteSkill, toggleSkill } = useApi()

  useEffect(() => {
    loadSkills()
  }, [])

  const loadSkills = async () => {
    try {
      const result = await listSkills()
      setSkills(result)
    } catch (error) {
      console.error('Failed to load skills:', error)
    }
  }

  const handleToggle = async (id: string) => {
    try {
      const enabled = await toggleSkill(id)
      setSkills(prev => prev.map(s => s.id === id ? { ...s, enabled } : s))
    } catch (error) {
      console.error('Failed to toggle skill:', error)
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deleteSkill(id)
      setSkills(prev => prev.filter(s => s.id !== id))
    } catch (error) {
      console.error('Failed to delete skill:', error)
    }
  }

  const getLevelLabel = (level: number) => {
    switch (level) {
      case 0: return '基础'
      case 1: return '进阶'
      case 2: return '高级'
      default: return '未知'
    }
  }

  const getLevelColor = (level: number) => {
    switch (level) {
      case 0: return 'bg-green-100 text-green-800'
      case 1: return 'bg-blue-100 text-blue-800'
      case 2: return 'bg-purple-100 text-purple-800'
      default: return 'bg-gray-100 text-gray-800'
    }
  }

  return (
    <div className="p-4">
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-xl font-bold flex items-center gap-2">
          <Wrench className="w-6 h-6" />
          技能管理
        </h1>
      </div>

      <Card>
        <CardContent className="p-6">
          <div className="space-y-4">
            {skills.length === 0 ? (
              <div className="text-center py-12 text-muted-foreground">
                <Wrench className="w-16 h-16 mx-auto mb-4 opacity-50" />
                <p>暂无技能</p>
              </div>
            ) : (
              <div className="grid gap-4">
                {skills.map((skill) => (
                  <div
                    key={skill.id}
                    className="flex items-center justify-between p-4 bg-muted rounded-lg"
                  >
                    <div className="flex items-center gap-4">
                      <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center">
                        <Wrench className="w-6 h-6 text-primary" />
                      </div>
                      <div>
                        <h3 className="font-semibold">{skill.name}</h3>
                        <p className="text-sm text-muted-foreground">{skill.description}</p>
                        <div className="flex items-center gap-2 mt-2">
                          <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${getLevelColor(skill.level)}`}>
                            L{skill.level} {getLevelLabel(skill.level)}
                          </span>
                          <span className="text-xs text-muted-foreground">v{skill.version}</span>
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-3">
                      <button
                        className={`p-2 rounded-lg transition-colors cursor-pointer ${skill.enabled ? 'bg-green-100 text-green-600 hover:bg-green-200' : 'bg-muted-foreground/10 text-muted-foreground hover:bg-muted-foreground/20'}`}
                        onClick={() => handleToggle(skill.id)}
                        title={skill.enabled ? '点击停用' : '点击启用'}
                      >
                        {skill.enabled ? <ToggleRight className="w-5 h-5" /> : <ToggleLeft className="w-5 h-5" />}
                      </button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleDelete(skill.id)}
                      >
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}