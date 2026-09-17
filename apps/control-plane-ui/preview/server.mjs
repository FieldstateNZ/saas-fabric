/**
 * The local workbench's dev server.
 *
 * Loopback only, and proxied to a real running control-plane API example
 * (`cargo run -p fabric-control-plane-api --example console_workbench`) —
 * see PHASE_ONE.md. OIDC is the only posture production configuration
 * accepts; there is no trusted-header mode for this to stand in for.
 * `X-Test-Operator` is accepted only by `testing::AcceptingOperator`, in
 * `crates/fabric-control-plane/src/testing.rs`, which the example composes
 * in and which treats any request carrying it as a fixed operator — a
 * development shortcut recorded as a proposal in ADR 0021 §7, not something
 * a deployment could configure.
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
        // Explicit rather than left to Vite's default. The workbench API answers
        // only when Host is its own address (its DNS-rebinding defence), and this
        // forwards the target's host. Vite 6.4 already does, and Vite's own
        // allowedHosts check refuses a foreign Host on :5174 with 403 before the
        // proxy runs — both observed against a stub target, 2026-09-16.
        changeOrigin: true,
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
