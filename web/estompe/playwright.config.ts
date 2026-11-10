import { defineConfig } from '@playwright/test'

// Test de bout en bout : voir e2e/global-setup.ts (Docker et Go requis).
export default defineConfig({
  testDir: './e2e',
  testIgnore: 'entracte.spec.ts',
  globalSetup: './e2e/global-setup.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: { launchOptions: { args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] }, baseURL: 'http://localhost:4173', trace: 'retain-on-failure', locale: 'fr-FR' },
  webServer: {
    command: 'VITE_API_URL=http://localhost:18080 VITE_GATEWAY_URL=http://localhost:19000 npm run build && npx vite preview --port 4173 --strictPort',
    url: 'http://localhost:4173',
    reuseExistingServer: false,
    timeout: 120_000,
  },
})
