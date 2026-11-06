/** Connexion Android : Authorization Code + PKCE dans le navigateur système, redirection par schéma d'URL ; le refresh token vit côté Kotlin. */
import { setTokenProvider } from '../../frontend/src/lib/api'
import { config } from '../../frontend/src/lib/config'
import { completeLogin, localSecretStore, TokenManager, type PendingLogin, type SecretStore } from '../../crates/core/ts/desktop-auth'
import type { AuthDriver } from '../../frontend/src/lib/platform'
import { notify } from '../../frontend/src/lib/ui'
import { browserLogin, diagnose, nativeFetch } from '../../crates/core/ts/tauri'
import { nativeBridge } from './bridge'

const SECRET_KEY = 'refresh-token'
const PENDING_KEY = 'connexion-en-cours'
const PENDING_TTL_MS = 10 * 60_000

/** La redirection passe par un schéma d'URL (un serveur loopback ne survit pas à la mise en arrière-plan de l'app). */
const REDIRECT = 'fr.plbls.estompe:/callback'

/** Le refresh token vit côté Kotlin, que les actions des notifications utilisent aussi (il tourne à chaque usage, une seule copie). */
const nativeSecretStore = (): SecretStore => ({
  load: async () => {
    const b = nativeBridge()
    const t = b?.getRefreshToken()
    if (t) return t
    // migration : session créée avant que le refresh token ne soit gardé côté Kotlin
    const old = localSecretStore(SECRET_KEY)
    const legacy = await old.load()
    if (legacy) { b?.saveRefreshToken(legacy); await old.clear() }
    return legacy
  },
  save: async (v) => nativeBridge()?.saveRefreshToken(v),
  clear: async () => nativeBridge()?.clearRefreshToken(),
})

const queryOf = (url: string) => url.split('?')[1] ?? ''

/** Attend que le navigateur rouvre l'app avec la redirection ; renvoie sa query string. */
async function waitCallback(): Promise<string> {
  for (let i = 0; i < 1500; i++) { // 5 minutes
    const url = nativeBridge()?.takeAuthUrl()
    if (url) return queryOf(url)
    await new Promise((r) => setTimeout(r, 200))
  }
  throw new Error('délai de connexion dépassé')
}

function savePending(p: PendingLogin) {
  try { localStorage.setItem(PENDING_KEY, JSON.stringify({ ...p, at: Date.now() })) } catch {}
}

function takePending(): PendingLogin | null {
  try {
    const raw = localStorage.getItem(PENDING_KEY)
    localStorage.removeItem(PENDING_KEY)
    const p = raw ? JSON.parse(raw) : null
    return p && Date.now() - p.at < PENDING_TTL_MS ? { state: p.state, verifier: p.verifier, redirectUri: p.redirectUri } : null
  } catch {
    return null
  }
}

async function resumeLogin(m: TokenManager) {
  const url = nativeBridge()?.takeAuthUrl()
  const pending = takePending()
  if (!url || !pending) return
  try {
    await completeLogin(m, queryOf(url), pending)
  } catch (e) {
    notify.fail(`Connexion impossible : ${e instanceof Error ? e.message : String(e)}`)
  }
}

let manager: TokenManager | null = null
let resumed: Promise<void> = Promise.resolve()

export const auth: AuthDriver = {
  setup() {
    if (manager) return
    nativeBridge()?.configure(config.keycloakIssuer, config.apiUrl)
    const store = nativeBridge() ? nativeSecretStore() : localSecretStore(SECRET_KEY)
    manager = new TokenManager({ issuer: config.keycloakIssuer, clientId: 'plbls-android', store, fetch: nativeFetch })
    resumed = resumeLogin(manager)
    setTokenProvider(async () => { await resumed; return manager!.getAccessToken() })
  },
  async login(register) {
    if (!manager) throw new Error('application Android non initialisée')
    nativeBridge()?.takeAuthUrl()
    try {
      await browserLogin(manager, { issuer: config.keycloakIssuer, clientId: 'plbls-android', redirectUri: REDIRECT, register, wait: waitCallback, onStart: savePending })
    } finally {
      takePending()
    }
  },
  diagnose: () => diagnose(manager),
  logout: async () => { await manager?.logout() },
}
