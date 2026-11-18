import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  testMatch: 'sablier.spec.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4177', locale: 'fr-FR', serviceWorkers: 'block', trace: 'retain-on-failure',
    launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
  },
  webServer: {
    command: 'VITE_API_URL=http://estompe.test VITE_GATEWAY_URL=http://gateway.test VITE_ENTRACTE_URL=http://entracte.test VITE_SILLON_URL=http://sillon.test VITE_SABLIER_URL=http://sablier.test npm run build -- --outDir dist/e2e-sablier && npx vite preview --outDir dist/e2e-sablier --port 4177 --strictPort',
    url: 'http://localhost:4177',
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
