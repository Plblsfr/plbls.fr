import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  testMatch: 'regie.spec.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4176', locale: 'fr-FR', serviceWorkers: 'block', trace: 'retain-on-failure',
    launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
  },
  webServer: {
    command: 'VITE_API_URL=http://estompe.test VITE_GATEWAY_URL=http://gateway.test VITE_REGIE_URL=http://regie.test npm run build -- --outDir dist/e2e-regie && npx vite preview --outDir dist/e2e-regie --port 4176 --strictPort',
    url: 'http://localhost:4176',
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
