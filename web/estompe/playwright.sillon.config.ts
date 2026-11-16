import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  testMatch: 'sillon.spec.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4175', locale: 'fr-FR', serviceWorkers: 'block', trace: 'retain-on-failure',
    launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
  },
  webServer: {
    command: 'VITE_API_URL=http://estompe.test VITE_GATEWAY_URL=http://gateway.test VITE_ENTRACTE_URL=http://entracte.test VITE_SILLON_URL=http://sillon.test npm run build -- --outDir dist/e2e-sillon && npx vite preview --outDir dist/e2e-sillon --port 4175 --strictPort',
    url: 'http://localhost:4175',
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
