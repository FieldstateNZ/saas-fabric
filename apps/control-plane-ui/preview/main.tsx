/**
 * The entry point for the local workbench preview (`npm run preview:ui`).
 *
 * Renders the real {@link Console} against a real, running control-plane API
 * — see `preview/server.mjs` for the proxy that makes that true. The banner
 * is the only thing this entry adds: everything below it is the production
 * console, unmodified, so what an operator sees here is what they would see
 * anywhere else this console is deployed.
 */
import { createRoot } from 'react-dom/client'

import { Console } from '../src/console/Console'
import '../src/styles.css'
import '../src/console/fonts.css'
import '../src/console/console.css'

const root = document.getElementById('root')

if (root) {
  createRoot(root).render(
    <>
      <div className="preview-banner">
        Local workbench · Persistent data and real APIs · External providers are not connected.
      </div>
      <Console />
    </>,
  )
}
