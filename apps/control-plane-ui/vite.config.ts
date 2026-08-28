import react from '@vitejs/plugin-react'
// `vitest/config` rather than `vite`: it is the same `defineConfig` with the
// `test` section typed, so the test configuration is checked rather than
// silently accepted as an unknown key.
import { defineConfig } from 'vitest/config'

/**
 * The operator identity the dev proxy presents.
 *
 * In production the operator-plane proxy authenticates the human and states
 * who they are in this header; the browser never sets it and could not be
 * trusted to. Locally there is no such proxy, so the dev server plays the same
 * role — which keeps the application code identical in both environments,
 * rather than growing a "development mode" that behaves differently from the
 * thing being shipped.
 */
const devOperator = process.env.VITE_DEV_OPERATOR ?? 'operator@example.com'
const controlPlane = process.env.VITE_CONTROL_PLANE ?? 'http://localhost:8081'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: controlPlane,
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyReq', (request) => {
            request.setHeader('Tailscale-User-Login', devOperator)
          })
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
  },
})
