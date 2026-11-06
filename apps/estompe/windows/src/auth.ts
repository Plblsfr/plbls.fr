/** Connexion Windows : Authorization Code + PKCE dans le navigateur système, redirection sur un serveur loopback local (RFC 8252) ; jeton d'accès en mémoire, refresh token dans le coffre de l'OS. */
import { setTokenProvider } from '../../../../web/estompe/src/lib/api'
import { config } from '../../../../web/estompe/src/lib/config'
import { TokenManager } from '../../crates/core/ts/desktop-auth'
import type { AuthDriver } from '../../../../web/estompe/src/lib/platform'
import { browserLogin, diagnose, invoke, nativeFetch } from '../../crates/core/ts/tauri'

const SECRET_KEY = 'refresh-token'
const CLIENT_ID = 'plbls-desktop'

let manager: TokenManager | null = null

export const auth: AuthDriver = {
  setup() {
    if (manager) return
    const store = {
      load: async () => (await invoke<string | null>('secret_get', { key: SECRET_KEY })) ?? null,
      save: (v: string) => invoke('secret_set', { key: SECRET_KEY, value: v }),
      clear: () => invoke('secret_delete', { key: SECRET_KEY }),
    }
    manager = new TokenManager({ issuer: config.keycloakIssuer, clientId: CLIENT_ID, store, fetch: nativeFetch })
    setTokenProvider(() => manager!.getAccessToken())
  },
  async login(register) {
    if (!manager) throw new Error('application Windows non initialisée')
    const redirectUri = `http://127.0.0.1:${await invoke<number>('oauth_start')}/callback`
    await browserLogin(manager, { issuer: config.keycloakIssuer, clientId: CLIENT_ID, redirectUri, register, wait: () => invoke<string>('oauth_wait') })
  },
  diagnose: () => diagnose(manager),
  logout: async () => { await manager?.logout() },
}
