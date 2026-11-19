import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import '@fontsource/doto/700.css'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { IS_REMOTE } from './api'
import './styles.css'

// On a paired device only: the worker that shows raiox's notifications.
if (IS_REMOTE && 'serviceWorker' in navigator) {
  navigator.serviceWorker.register('/sw.js').catch(() => undefined)
}

const root = document.getElementById('root')
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
}
