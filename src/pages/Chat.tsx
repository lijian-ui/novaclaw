import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { ChatPanel } from '@/components/ChatPanel'
import { SessionList } from '@/components/SessionList'
import { ModelSelector } from '@/components/ModelSelector'
import { MessageSquare } from 'lucide-react'

export function Chat() {
  return (
    <div className="h-full">
      <div className="grid grid-cols-4 gap-4 p-4">
        <Card className="col-span-1 h-[calc(100vh-100px)]">
          <CardContent className="p-0 h-full">
            <SessionList />
          </CardContent>
        </Card>

        <Card className="col-span-3 h-[calc(100vh-100px)]">
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-base flex items-center gap-2">
              <MessageSquare className="w-5 h-5" />
              聊天
            </CardTitle>
            <ModelSelector />
          </CardHeader>
          <CardContent className="p-0 h-[calc(100%-60px)]">
            <ChatPanel />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}