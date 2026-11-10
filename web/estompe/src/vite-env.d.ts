/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** URL de l'API Estompe (ex. https://chat.plbls.fr). Vide = même origine. */
  readonly VITE_API_URL?: string
  /** URL d'auth-gateway (ex. https://security.plbls.fr). */
  readonly VITE_GATEWAY_URL?: string
  /** Émetteur Keycloak pour la connexion de l'app Windows. */
  readonly VITE_KEYCLOAK_ISSUER?: string
  /** URL de l'API d'Entracte (ex. https://entracte-api.plbls.fr) ; vide = pas de section Entracte. */
  readonly VITE_ENTRACTE_URL?: string
}

declare module 'virtual:pwa-register' {
  export function registerSW(options?: { immediate?: boolean }): (reloadPage?: boolean) => Promise<void>
}

declare module '@sapphi-red/web-noise-suppressor/*?url' {
  const url: string
  export default url
}

declare const __APP_VERSION__: string
