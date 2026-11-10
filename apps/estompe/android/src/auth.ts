/** Connexion Android : Authorization Code + PKCE dans le navigateur système, redirection par schéma d'URL ; le refresh token vit côté Kotlin et n'arrive jamais ici. */
import { setTokenProvider } from '../../../../web/estompe/src/lib/api'
import { config } from '../../../../web/estompe/src/lib/config'
import { completeLogin, localSecretStore, TokenManager, type PendingLogin, type Tokens } from '../../crates/core/ts/desktop-auth'
import type { AuthDriver } from '../../../../web/estompe/src/lib/platform'
import { notify } from '../../../../web/estompe/src/lib/ui'
import { browserLogin, diagnose, nativeFetch } from '../../crates/core/ts/tauri'
import { nativeBridge, nativeSession, type SessionCall, type SessionReply } from './bridge'

const SECRET_KEY = 'refresh-token'
const PENDING_KEY = 'connexion-en-cours'
const PENDING_TTL_MS = 10 * 60_000

/** La redirection passe par un schéma d'URL (un serveur loopback ne survit pas à la mise en arrière-plan de l'app). */
const REDIRECT = 'fr.plbls.estompe:/callback'

const LEEWAY_MS = 30_000

const failure = (r: SessionReply) => Object.assign(new Error(r.error || `HTTP ${r.status}`), { status: r.status })

/** Le refresh token vit côté Kotlin, que les actions des notifications utilisent aussi (il tourne à chaque usage, une seule copie) : le front n'en reçoit que des jetons d'accès. */
export class NativeTokens implements Tokens {
  private access: string | null = null
  private expiresAt = 0
  private inflight: Promise<string | null> | null = null

  constructor(private call: SessionCall, private now: () => number = Date.now) {}

  private accept(r: SessionReply): string | null {
    this.access = r.access_token || null
    this.expiresAt = this.now() + (r.expires_in ?? 0) * 1000
    return this.access
  }

  async loginWithCode(code: string, verifier: string, redirectUri: string) {
    const r = await this.call({ op: 'login', code, verifier, redirectUri })
    if (r.status !== 200 || !r.access_token) throw failure(r)
    this.accept(r)
  }

  getAccessToken(): Promise<string | null> {
    if (this.access && this.now() < this.expiresAt - LEEWAY_MS) return Promise.resolve(this.access)
    this.inflight ??= this.renew().finally(() => (this.inflight = null))
    return this.inflight
  }

  private async renew(): Promise<string | null> {
    const r = await this.call({ op: 'renew' })
    if (r.status === 200 && r.access_token) return this.accept(r)
    if (r.status === 400 || r.status === 401) { this.access = null; return null }
    if (this.access && this.now() < this.expiresAt) return this.access
    throw failure(r)
  }

  async logout() {
    this.access = null
    this.expiresAt = 0
    await this.call({ op: 'logout' })
  }
}

async function adoptLegacy(call: SessionCall) {
  const old = localSecretStore(SECRET_KEY)
  const legacy = await old.load()
  if (!legacy) return
  await call({ op: 'adopt', refresh: legacy })
  await old.clear()
}

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

async function resumeLogin(m: Tokens) {
  const url = nativeBridge()?.takeAuthUrl()
  const pending = takePending()
  if (!url || !pending) return
  try {
    await completeLogin(m, queryOf(url), pending)
  } catch (e) {
    notify.fail(`Connexion impossible : ${e instanceof Error ? e.message : String(e)}`)
  }
}

let manager: Tokens | null = null
let resumed: Promise<void> = Promise.resolve()

export const auth: AuthDriver = {
  setup() {
    if (manager) return
    const call = nativeSession()
    const m = call ? new NativeTokens(call) : new TokenManager({ issuer: config.keycloakIssuer, clientId: 'plbls-android', store: localSecretStore(SECRET_KEY), fetch: nativeFetch })
    manager = m
    resumed = (call ? adoptLegacy(call) : Promise.resolve()).then(() => resumeLogin(m))
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
