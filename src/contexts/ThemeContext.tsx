import { createContext, useContext, useEffect, useState, useCallback, type ReactNode } from 'react'

type Theme = 'dark' | 'light' | 'system'

interface ThemeContextValue {
  theme: Theme
  toggle: () => void
  setTheme: (t: Theme) => void
  isDark: boolean
  isLight: boolean
}

const ThemeContext = createContext<ThemeContextValue>({
  theme: 'dark',
  toggle: () => {},
  setTheme: () => {},
  isDark: true,
  isLight: false,
})

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => {
    const saved = localStorage.getItem('jeeves-theme')
    return (saved === 'light' || saved === 'dark' || saved === 'system') ? saved : 'dark'
  })

  const getSystemTheme = (): Theme => {
    if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
      return 'dark'
    }
    return 'light'
  }

  const getEffectiveTheme = useCallback((): Theme => {
    if (theme === 'system') {
      return getSystemTheme()
    }
    return theme
  }, [theme])

  const applyTheme = useCallback((t: Theme) => {
    const effectiveTheme = t === 'system' ? getSystemTheme() : t
    document.documentElement.classList.remove('dark', 'light')
    document.documentElement.classList.add(effectiveTheme)
    localStorage.setItem('jeeves-theme', t)
  }, [])

  useEffect(() => {
    applyTheme(theme)
  }, [theme, applyTheme])

  useEffect(() => {
    if (theme !== 'system') return
    
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => {
      applyTheme('system')
    }
    
    mediaQuery.addEventListener('change', handleChange)
    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [theme, applyTheme])

  const toggle = useCallback(() => {
    setThemeState(prev => prev === 'dark' ? 'light' : 'dark')
  }, [])

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t)
  }, [])

  const effectiveTheme = getEffectiveTheme()

  return (
    <ThemeContext.Provider value={{ 
      theme, 
      toggle, 
      setTheme, 
      isDark: effectiveTheme === 'dark', 
      isLight: effectiveTheme === 'light' 
    }}>
      {children}
    </ThemeContext.Provider>
  )
}

export function useTheme() {
  return useContext(ThemeContext)
}
