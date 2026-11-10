import { defineConfig } from '@playwright/test'

// Sections de l'app selon les permissions (Entracte) : les API d'Estompe et d'Entracte sont simulées dans la page
// (page.route), sans Docker ni backend. Lancement : npm run e2e:entracte --workspace web/estompe
export default defineConfig({
  testDir: './e2e',
  testMatch: 'entracte.spec.ts',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:4174', locale: 'fr-FR', serviceWorkers: 'block', trace: 'retain-on-failure',
    launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
  },
  webServer: {
    command: 'VITE_API_URL=http://estompe.test VITE_GATEWAY_URL=http://gateway.test VITE_ENTRACTE_URL=http://entracte.test npm run build -- --outDir dist/e2e-entracte && npx vite preview --outDir dist/e2e-entracte --port 4174 --strictPort',
    url: 'http://localhost:4174',
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
