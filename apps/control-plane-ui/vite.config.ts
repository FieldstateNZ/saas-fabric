import react from '@vitejs/plugin-react'
// `vitest/config` rather than `vite`: it is the same `defineConfig` with the
// `test` section typed, so the test configuration is checked rather than
// silently accepted as an unknown key.
import { defineConfig } from 'vitest/config'

const controlPlane = process.env.VITE_CONTROL_PLANE ?? 'http://localhost:8081'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: controlPlane,
        changeOrigin: true,
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
