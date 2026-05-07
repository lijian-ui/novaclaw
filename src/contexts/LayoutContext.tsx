import { createContext, useContext, useState, useCallback, ReactNode, useEffect } from 'react'
import type { Layout } from '@/types'
import { useApi } from '@/hooks/useApi'

interface LayoutContextType {
  layout: Layout | null
  setLayout: (layout: Layout | null) => void
  saveLayout: (name: string, content: string) => Promise<void>
  loadLayout: () => Promise<void>
}

const LayoutContext = createContext<LayoutContextType | null>(null)

export function LayoutProvider({ children }: { children: ReactNode }) {
  const [layout, setLayout] = useState<Layout | null>(null)
  const { getLayout: fetchLayout, saveLayout: saveLayoutApi } = useApi()

  const saveLayout = useCallback(async (name: string, content: string) => {
    try {
      const result = await saveLayoutApi(name, content)
      setLayout(result)
    } catch {
      console.error('Failed to save layout')
    }
  }, [saveLayoutApi])

  const loadLayout = useCallback(async () => {
    try {
      const result = await fetchLayout()
      setLayout(result)
    } catch {
      console.error('Failed to load layout')
    }
  }, [fetchLayout])

  useEffect(() => {
    loadLayout()
  }, [loadLayout])

  return (
    <LayoutContext.Provider
      value={{
        layout,
        setLayout,
        saveLayout,
        loadLayout,
      }}
    >
      {children}
    </LayoutContext.Provider>
  )
}

export function useLayout() {
  const context = useContext(LayoutContext)
  if (!context) {
    throw new Error('useLayout must be used within a LayoutProvider')
  }
  return context
}