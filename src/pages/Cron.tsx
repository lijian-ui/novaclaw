import { useState, useEffect } from 'react'
import { Card, CardContent } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { useApi } from '@/hooks/useApi'
import type { CronJob } from '@/types'
import { Clock, Trash2, Play, Pause } from 'lucide-react'

export function Cron() {
  const [jobs, setJobs] = useState<CronJob[]>([])
  const { listCronJobs, deleteCronJob } = useApi()

  useEffect(() => {
    loadJobs()
  }, [])

  const loadJobs = async () => {
    try {
      const result = await listCronJobs()
      setJobs(result)
    } catch (error) {
      console.error('Failed to load cron jobs:', error)
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deleteCronJob(id)
      setJobs(prev => prev.filter(j => j.id !== id))
    } catch (error) {
      console.error('Failed to delete cron job:', error)
    }
  }

  const toggleJob = (job: CronJob) => {
    console.log('Toggle job:', job.id, !job.enabled)
  }

  const formatSchedule = (schedule: string) => {
    if (schedule.startsWith('@')) {
      return schedule.replace('@daily', '每天').replace('@hourly', '每小时').replace('@weekly', '每周')
    }
    return schedule
  }

  return (
    <div className="p-4">
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-xl font-bold flex items-center gap-2">
          <Clock className="w-6 h-6" />
          定时任务
        </h1>
        <Button onClick={() => console.log('Create job')}>
          添加任务
        </Button>
      </div>

      <Card>
        <CardContent className="p-6">
          <div className="space-y-4">
            {jobs.length === 0 ? (
              <div className="text-center py-12 text-muted-foreground">
                <Clock className="w-16 h-16 mx-auto mb-4 opacity-50" />
                <p>暂无定时任务</p>
              </div>
            ) : (
              <div className="grid gap-4">
                {jobs.map((job) => (
                  <div
                    key={job.id}
                    className="flex items-center justify-between p-4 bg-muted rounded-lg"
                  >
                    <div className="flex items-center gap-4">
                      <div className={`w-12 h-12 rounded-lg flex items-center justify-center ${job.enabled ? 'bg-green-100' : 'bg-muted-foreground/10'}`}>
                        <Clock className={`w-6 h-6 ${job.enabled ? 'text-green-600' : 'text-muted-foreground'}`} />
                      </div>
                      <div>
                        <h3 className="font-semibold">{job.name}</h3>
                        <p className="text-sm text-muted-foreground">调度: {formatSchedule(job.schedule)}</p>
                        <p className="text-xs text-muted-foreground mt-1">
                          创建于 {new Date(job.created_at).toLocaleDateString('zh-CN')}
                        </p>
                      </div>
                    </div>
                    <div className="flex items-center gap-3">
                      <span className={`px-2 py-1 rounded-full text-xs font-medium ${job.enabled ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}`}>
                        {job.enabled ? '运行中' : '已停止'}
                      </span>
                      <button
                        className={`p-2 rounded-lg transition-colors ${job.enabled ? 'bg-yellow-100 text-yellow-600 hover:bg-yellow-200' : 'bg-green-100 text-green-600 hover:bg-green-200'}`}
                        onClick={() => toggleJob(job)}
                      >
                        {job.enabled ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5" />}
                      </button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleDelete(job.id)}
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