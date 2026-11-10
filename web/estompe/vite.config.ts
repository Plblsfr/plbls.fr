import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { VitePWA } from 'vite-plugin-pwa'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

// Version de l'app : celle de tauri.conf.json (la CI y écrit celle de .version avant le build).
const appVersion: string = JSON.parse(readFileSync('../../apps/estompe/windows/src-tauri/tauri.conf.json', 'utf8')).version

// Chaque plateforme (web, windows, android) a son dossier avec son `src/platform.ts` ; le mode de build la choisit (`--mode windows`).
// web : plateforme/ (image nginx) ; windows et android : apps/estompe/<plateforme> (coques Tauri).
const platformOf = (mode: string) => (mode === 'windows' || mode === 'android' ? `../../apps/estompe/${mode}` : './plateforme')
const dir = (p: string) => fileURLToPath(new URL(p, import.meta.url))

export default defineConfig(({ mode }) => ({
  plugins: [
    svelte(),
    VitePWA({
      registerType: 'autoUpdate',
      injectRegister: mode === 'windows' || mode === 'android' ? false : 'auto',
      selfDestroying: mode === 'windows' || mode === 'android',
      manifest: {
        name: 'Estompe',
        short_name: 'Estompe',
        description: 'Messagerie écrite et vocale',
        lang: 'fr',
        start_url: '/',
        display: 'standalone',
        background_color: '#f7f3ee',
        theme_color: '#f7f3ee',
        icons: [
          { src: 'icon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
          { src: 'icon-maskable.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'maskable' },
        ],
      },
      workbox: {
        importScripts: ['push-sw.js'],
        globPatterns: ['**/*.{js,css,svg,woff2}'],
        globIgnores: ['config.js'],
        navigateFallback: null,
        runtimeCaching: [{
          urlPattern: ({ request, url }) => url.pathname === '/config.js' || (request.mode === 'navigate' && !/^\/(api|updates)\//.test(url.pathname)),
          handler: 'NetworkFirst',
          options: { cacheName: 'estompe-pages', networkTimeoutSeconds: 5 },
        }],
      },
    }),
  ],
  define: { __APP_VERSION__: JSON.stringify(appVersion) },
  build: { target: 'es2022', sourcemap: false },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.test.ts', 'plateforme/src/**/*.test.ts', '../../apps/estompe/windows/src/**/*.test.ts', '../../apps/estompe/android/src/**/*.test.ts', '../../apps/estompe/crates/core/ts/**/*.test.ts'],
    css: false,
    testTimeout: 20000, // runners CI lents : le 1er rendu à froid dépasse 5 s
  },
  resolve: {
    conditions: ['browser'],
    alias: { '@platform': dir(`${platformOf(mode)}/src/platform.ts`) },
    // le code des apps natives vit hors de web/estompe/ : ses dépendances se résolvent depuis le node_modules du workspace
    dedupe: ['svelte', '@tauri-apps/api', '@tauri-apps/plugin-opener', '@tauri-apps/plugin-notification', '@tauri-apps/plugin-updater', '@tauri-apps/plugin-global-shortcut'],
  },
  server: { fs: { allow: ['../..'] } },
}))
