import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter } from 'react-router-dom'
import App from './App'
import { ThemeProvider } from './contexts/ThemeContext'
import { ChatProvider } from './contexts/ChatContext'
import './i18n'
import './index.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <HashRouter>
      <ThemeProvider>
        <ChatProvider>
          <App />
        </ChatProvider>
      </ThemeProvider>
    </HashRouter>
  </StrictMode>,
)