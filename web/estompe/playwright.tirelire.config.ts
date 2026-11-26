import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  testMatch: 'tirelire.spec.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4179', locale: 'fr-FR', timezoneId: 'Europe/Paris', colorScheme: 'dark', serviceWorkers: 'block', trace: 'retain-on-failure',
    launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
  },
  webServer: {
    command: 'VITE_API_URL=http://estompe.test VITE_GATEWAY_URL=http://gateway.test VITE_ENTRACTE_URL=http://entracte.test VITE_SILLON_URL=http://sillon.test VITE_SABLIER_URL=http://sablier.test VITE_TIRELIRE_URL=http://tirelire.test npm run build -- --outDir dist/e2e-tirelire && npx vite preview --outDir dist/e2e-tirelire --port 4179 --strictPort',
    url: 'http://localhost:4179',
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
