import { createRoot } from 'react-dom/client'
import { Console } from '../src/console/Console'
import '../src/styles.css'
import '../src/console/fonts.css'
import '../src/console/console.css'

const root = document.getElementById('root')
if (root) createRoot(root).render(<><div className="preview-banner">Local workbench · Persistent data and real APIs · External providers are not connected.</div><Console /></>)
