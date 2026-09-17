/**
 * The entry point for the local workbench preview (`npm run preview:ui`).
 *
 * Renders the real {@link Console} component against a real, running
 * control-plane API — see `preview/server.mjs` for the proxy that makes that
 * true. It is not what an operator would see anywhere else this console is
 * deployed: there is no sign-in here (`App.tsx`'s `useSession` gate is
 * skipped entirely), and the workbench connects no identity provider, secret
 * store, Git integration, or platform management — every one of them is
 * `None` behind the example API this proxies to (ADR 0021 §7).
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
