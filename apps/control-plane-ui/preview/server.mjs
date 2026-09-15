/**
 * The local workbench's dev server.
 *
 * Loopback only, and proxied to a real running control-plane API example
 * (`cargo run -p fabric-control-plane-api --example console_workbench`) —
 * see PHASE_ONE.md. `X-Test-Operator` stands in for the trusted-header
 * identity a production proxy would set; the production entry point keeps
 * OIDC and never sets this header itself.
 */
import { fileURLToPath } from 'node:url'

import { createServer } from 'vite'

const root = fileURLToPath(new URL('../', import.meta.url))

const server = await createServer({
  root,
  server: {
    host: '127.0.0.1',
    port: 5174,
    strictPort: true,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8082',
        headers: { 'X-Test-Operator': 'local-workbench' },
      },
    },
  },
  plugins: [
    {
      name: 'fabric-local-workbench',
      configureServer(vite) {
        // The workbench has its own HTML entry, not the production one — see
        // `preview.html` and `preview/main.tsx` — so a request for the root
        // document is rewritten before Vite resolves it.
        vite.middlewares.use((req, _res, next) => {
          if (req.url === '/') {
            req.url = '/preview.html'
          }
          next()
        })
      },
    },
  ],
})

await server.listen()
server.printUrls()
