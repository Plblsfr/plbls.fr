import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  testMatch: 'cinema.spec.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4178', locale: 'fr-FR', serviceWorkers: 'block', trace: 'retain-on-failure',
    launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
  },
  webServer: {
    command: 'VITE_API_URL=http://estompe.test VITE_GATEWAY_URL=http://gateway.test VITE_ENTRACTE_URL=http://entracte.test npx vite --port 4178 --strictPort',
    url: 'http://localhost:4178',
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
