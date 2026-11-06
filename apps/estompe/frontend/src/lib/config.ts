/** Configuration d'exécution : /config.js (écrasé au démarrage du conteneur), sinon variables Vite. */
interface RuntimeConfig {
  apiUrl?: string
  gatewayUrl?: string
  /** Émetteur Keycloak (ex. https://auth.plbls.fr/realms/default), pour la connexion PKCE de l'app Windows. */
  keycloakIssuer?: string
}

const rc: RuntimeConfig = (typeof window !== 'undefined' && (window as unknown as { __ESTOMPE__?: RuntimeConfig }).__ESTOMPE__) || {}
const strip = (s: string) => s.replace(/\/$/, '')

export const config = {
  apiUrl: strip(rc.apiUrl || import.meta.env.VITE_API_URL || ''),
  gatewayUrl: strip(rc.gatewayUrl || import.meta.env.VITE_GATEWAY_URL || 'https://security.plbls.fr'),
  keycloakIssuer: strip(rc.keycloakIssuer || import.meta.env.VITE_KEYCLOAK_ISSUER || ''),
}
