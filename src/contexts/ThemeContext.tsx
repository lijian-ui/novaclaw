import { createContext, useContext, useEffect, useState, useCallback, type ReactNode } from 'react'

type Theme = 'dark' | 'light'

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
    const saved = localStorage.getItem('novaclaw-theme')
    return (saved === 'light' || saved === 'dark') ? saved : 'dark'
  })

  const applyTheme = useCallback((t: Theme) => {
    document.documentElement.classList.remove('dark', 'light')
    document.documentElement.classList.add(t)
    localStorage.setItem('novaclaw-theme', t)
  }, [])

  useEffect(() => {
    applyTheme(theme)
  }, [theme, applyTheme])

  const toggle = useCallback(() => {
    setThemeState(prev => prev === 'dark' ? 'light' : 'dark')
  }, [])

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t)
  }, [])

  return (
    <ThemeContext.Provider value={{ theme, toggle, setTheme, isDark: theme === 'dark', isLight: theme === 'light' }}>
      {children}
    </ThemeContext.Provider>
  )
}

export function useTheme() {
  return useContext(ThemeContext)
}
