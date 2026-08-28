import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { App } from './App'
import './styles.css'

const root = document.getElementById('root')

if (root === null) {
  // Nothing sensible to render into, and nowhere to render a message, so this
  // is the one place the console gives up loudly rather than degrading.
  throw new Error('the application root element is missing')
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
