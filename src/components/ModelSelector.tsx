import { useState, useEffect } from 'react'
import { ChevronDown, Cpu } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { useChat } from '@/contexts/ChatContext'
import { useApi } from '@/hooks/useApi'
import type { Model } from '@/types'
import xiaomiIcon from '@/assets/Xiaomi.png'
import customAiIcon from '@/assets/AI.png'

function getProviderIcon(providerName: string): string | undefined {
  const name = providerName.toLowerCase().replace(/[\s_-]/g, '')
  if (name === 'custom' || name.includes('自定义') || name.includes('custom')) return customAiIcon
  if (name.includes('xiaomi') || name.includes('mimo')) return xiaomiIcon
  return undefined
}

export function ModelSelector() {
  const [models, setModels] = useState<Model[]>([])
  const [isOpen, setIsOpen] = useState(false)
  const { currentModel, setCurrentModel } = useChat()
  const { listModels } = useApi()

  useEffect(() => {
    loadModels()
  }, [])

  const loadModels = async () => {
    try {
      const result = await listModels()
      if (Array.isArray(result)) {
        setModels(result)
        if (!currentModel && result.length > 0) {
          setCurrentModel(result[0])
        }
      }
    } catch (error) {
      console.error('Failed to load models:', error)
    }
  }

  const handleSelect = (model: Model) => {
    setCurrentModel(model)
    setIsOpen(false)
  }

  return (
    <div className="relative">
      <Button
        variant="outline"
        size="sm"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 min-w-[180px] justify-between"
      >
        {currentModel && getProviderIcon(currentModel.provider) ? (
          <img src={getProviderIcon(currentModel.provider)} className="w-4 h-4 rounded shrink-0" alt="" />
        ) : (
          <Cpu className="w-4 h-4" />
        )}
        <span>{currentModel?.name || '选择模型'}</span>
        <ChevronDown className={`w-4 h-4 transition-transform ${isOpen ? 'rotate-180' : ''}`} />
      </Button>

      {isOpen && (
        <div className="absolute top-full left-0 mt-2 w-full max-h-64 overflow-y-auto rounded-lg border border-border bg-background shadow-lg z-50">
          {models.map((model) => (
            <button
              key={model.id}
              onClick={() => handleSelect(model)}
              className={`w-full flex items-center gap-2 px-4 py-3 text-left transition-colors ${
                currentModel?.id === model.id
                  ? 'bg-primary text-primary-foreground'
                  : 'hover:bg-accent text-muted-foreground'
              }`}
            >
              {getProviderIcon(model.provider) ? (
                <img src={getProviderIcon(model.provider)} className="w-4 h-4 rounded shrink-0" alt="" />
              ) : (
                <Cpu className="w-4 h-4 shrink-0" />
              )}
              <div className="min-w-0">
                <p className="font-medium truncate">{model.name}</p>
                <p className="text-xs opacity-70 truncate">{model.provider} - {model.context_window.toLocaleString()} tokens</p>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}