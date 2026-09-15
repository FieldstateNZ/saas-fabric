/** Loopback workbench proxy to the real Rust API; production entry keeps OIDC. */
import { createServer } from 'vite'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const server = await createServer({ root, server: {
  host: '127.0.0.1', port: 5174, strictPort: true,
  proxy: { '/api': { target: 'http://127.0.0.1:8082', headers: { 'X-Test-Operator': 'local-workbench' } } },
}, plugins: [{ name: 'fabric-local-workbench', configureServer(vite) {
  vite.middlewares.use((req, _res, next) => { if (req.url === '/') req.url = '/preview.html'; next() })
} }] })
await server.listen()
server.printUrls()
