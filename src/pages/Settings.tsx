import { useState, useEffect } from 'react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { useApi } from '@/hooks/useApi'
import type { Config, ServerConfig, LlmConfig, SecurityConfig } from '@/types'
import { Settings as SettingsIcon, Save, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

export function Settings() {
  const { t } = useTranslation()
  const [config, setConfig] = useState<Config | null>(null)
  const [localConfig, setLocalConfig] = useState<Partial<Config>>({})
  const { getConfig, updateConfig } = useApi()

  useEffect(() => {
    loadConfig()
  }, [])

  useEffect(() => {
    if (config) {
      setLocalConfig({
        server: { ...config.server },
        llm: { ...config.llm },
        security: { ...config.security },
      })
    }
  }, [config])

  const loadConfig = async () => {
    try {
      const result = await getConfig()
      setConfig(result)
    } catch (error) {
      console.error('Failed to load config:', error)
    }
  }

  const handleSave = async () => {
    try {
      const result = await updateConfig(localConfig)
      setConfig(result)
    } catch (error) {
      console.error('Failed to save config:', error)
    }
  }

  const handleReset = () => {
    if (config) {
      setLocalConfig({
        server: { ...config.server },
        llm: { ...config.llm },
        security: { ...config.security },
      })
    }
  }

  if (!config) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-muted-foreground">{t('settingsPage.loading')}</div>
      </div>
    )
  }

  return (
    <div className="p-4 space-y-4 max-w-4xl">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-bold flex items-center gap-2">
          <SettingsIcon className="w-6 h-6" />
          {t('settingsPage.systemSettings')}
        </h1>
        <div className="flex gap-2">
          <Button variant="outline" onClick={handleReset}>
            <RefreshCw className="w-4 h-4 mr-1" />
            {t('settingsPage.reset')}
          </Button>
          <Button onClick={handleSave}>
            <Save className="w-4 h-4 mr-1" />
            {t('settingsPage.save')}
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t('settingsPage.serverSettings')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium mb-2">{t('settingsPage.port')}</label>
              <Input
                type="number"
                value={localConfig.server?.port ?? config.server.port}
                onChange={(e) => setLocalConfig(prev => ({
                  ...prev,
                  server: { ...prev.server as ServerConfig, port: parseInt(e.target.value) || 3000 }
                }))}
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-2">{t('settingsPage.host')}</label>
              <Input
                value={localConfig.server?.host ?? config.server.host}
                onChange={(e) => setLocalConfig(prev => ({
                  ...prev,
                  server: { ...prev.server as ServerConfig, host: e.target.value }
                }))}
              />
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settingsPage.llmSettings')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium mb-2">{t('settingsPage.timeout')}</label>
              <Input
                type="number"
                value={localConfig.llm?.timeout ?? config.llm.timeout}
                onChange={(e) => setLocalConfig(prev => ({
                  ...prev,
                  llm: { ...prev.llm as LlmConfig, timeout: parseInt(e.target.value) || 30 }
                }))}
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-2">{t('settingsPage.maxRetries')}</label>
              <Input
                type="number"
                value={localConfig.llm?.max_retries ?? config.llm.max_retries}
                onChange={(e) => setLocalConfig(prev => ({
                  ...prev,
                  llm: { ...prev.llm as LlmConfig, max_retries: parseInt(e.target.value) || 3 }
                }))}
              />
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settingsPage.securitySettings')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <label className="flex items-center gap-3">
            <input
              type="checkbox"
              checked={localConfig.security?.prompt_injection_protection ?? config.security.prompt_injection_protection}
              onChange={(e) => setLocalConfig(prev => ({
                ...prev,
                security: { ...prev.security as SecurityConfig, prompt_injection_protection: e.target.checked }
              }))}
              className="w-4 h-4 rounded"
            />
            <span className="text-sm font-medium">{t('settingsPage.enablePromptInjectionProtection')}</span>
          </label>
        </CardContent>
      </Card>
    </div>
  )
}