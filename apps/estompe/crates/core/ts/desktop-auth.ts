/** Connexion PKCE (RFC 7636 / RFC 8252) de l'app Windows directement auprès de Keycloak, sans passer par le BFF. */

const enc = new TextEncoder()
const b64url = (b: ArrayBuffer | Uint8Array) =>
  btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')

export function randomString(bytes = 32): string {
  return b64url(crypto.getRandomValues(new Uint8Array(bytes)))
}

export async function challengeS256(verifier: string): Promise<string> {
  return b64url(await crypto.subtle.digest('SHA-256', enc.encode(verifier)))
}

export function authUrl(o: { issuer: string; clientId: string; redirectUri: string; state: string; challenge: string; register?: boolean }): string {
  const q = new URLSearchParams({
    client_id: o.clientId, response_type: 'code', scope: 'openid', redirect_uri: o.redirectUri,
    state: o.state, code_challenge: o.challenge, code_challenge_method: 'S256',
  })
  return `${o.issuer}/protocol/openid-connect/${o.register ? 'registrations' : 'auth'}?${q}`
}

export interface PendingLogin { state: string; verifier: string; redirectUri: string }

export interface Tokens {
  loginWithCode(code: string, verifier: string, redirectUri: string): Promise<void>
  getAccessToken(): Promise<string | null>
  logout(): Promise<void>
}

export async function completeLogin(manager: Tokens, query: string, pending: PendingLogin): Promise<void> {
  const q = new URLSearchParams(query)
  if (q.get('state') !== pending.state) throw new Error('Réponse de connexion inattendue (state)')
  const code = q.get('code')
  if (!code) throw new Error(q.get('error_description') ?? q.get('error') ?? 'Connexion refusée')
  await manager.loginWithCode(code, pending.verifier, pending.redirectUri)
}

/** Où garder le refresh token : le coffre de l'OS (Credential Manager) dans l'app. */
export interface SecretStore {
  load(): Promise<string | null>
  save(refreshToken: string): Promise<void>
  clear(): Promise<void>
}

/** Android : le keyring de Rust n'y a pas de coffre (stockage en mémoire, perdu à la fermeture) ; localStorage est privé à l'app. */
// shortcut: refresh token non chiffré dans le stockage de l'app, à passer par le Keystore Android si on veut plus.
export const localSecretStore = (key: string, ls: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> = localStorage): SecretStore => ({
  load: async () => ls.getItem(key),
  save: async (v) => ls.setItem(key, v),
  clear: async () => ls.removeItem(key),
})

interface TokenResponse { access_token: string; refresh_token?: string; expires_in: number }

const LEEWAY_MS = 30_000

/**
 * Détient le jeton d'accès en mémoire (jamais écrit sur disque) et le renouvelle avec le refresh token, qui tourne
 * à chaque usage (« Revoke Refresh Token » activé côté Keycloak) : le nouveau remplace l'ancien dans le coffre.
 */
export class TokenManager implements Tokens {
  private access: string | null = null
  private expiresAt = 0
  private inflight: Promise<string | null> | null = null

  constructor(
    private o: { issuer: string; clientId: string; store: SecretStore; fetch?: typeof fetch; now?: () => number },
  ) {}

  private get now() { return (this.o.now ?? Date.now)() }
  private get endpoint() { return `${this.o.issuer}/protocol/openid-connect/token` }
  private get f() { return this.o.fetch ?? fetch }

  private async token(body: Record<string, string>): Promise<TokenResponse> {
    const res = await this.f(this.endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ client_id: this.o.clientId, ...body }),
    })
    if (!res.ok) {
      const e = await res.json().catch(() => ({}))
      throw Object.assign(new Error(e.error_description ?? e.error ?? `HTTP ${res.status}`), { status: res.status, code: e.error as string | undefined })
    }
    return res.json()
  }

  private async accept(t: TokenResponse) {
    this.access = t.access_token
    this.expiresAt = this.now + t.expires_in * 1000
    if (t.refresh_token) await this.o.store.save(t.refresh_token)
  }

  /** Échange le code reçu sur la redirection loopback contre les jetons. */
  async loginWithCode(code: string, verifier: string, redirectUri: string) {
    await this.accept(await this.token({ grant_type: 'authorization_code', code, code_verifier: verifier, redirect_uri: redirectUri }))
  }

  /** Jeton d'accès valide, renouvelé si besoin ; null sans session (refresh token absent ou refusé) ; lève si le renouvellement échoue pour une raison passagère. */
  getAccessToken(): Promise<string | null> {
    if (this.access && this.now < this.expiresAt - LEEWAY_MS) return Promise.resolve(this.access)
    // Un seul renouvellement à la fois : le refresh token tourne, un second appel simultané le ferait refuser.
    this.inflight ??= this.refresh().finally(() => (this.inflight = null))
    return this.inflight
  }

  private async refresh(): Promise<string | null> {
    const rt = await this.o.store.load()
    if (!rt) return null
    try {
      await this.accept(await this.token({ grant_type: 'refresh_token', refresh_token: rt }))
      return this.access
    } catch (e: any) {
      if (e?.status === 400 || e?.status === 401) { // refusé : session terminée côté Keycloak
        this.access = null
        await this.o.store.clear()
        return null
      }
      // Réseau ou panne de Keycloak : on garde le refresh token, le prochain appel réessaiera. Ce n'est pas une
      // perte de session : on lève l'erreur plutôt que d'envoyer la requête sans jeton (401, écran de connexion).
      if (this.access && this.now < this.expiresAt) return this.access
      throw e
    }
  }

  async hasSession(): Promise<boolean> {
    return (await this.getAccessToken()) !== null
  }

  /** Révoque la session côté Keycloak (au mieux), puis oublie tout. */
  async logout() {
    const rt = await this.o.store.load()
    this.access = null
    this.expiresAt = 0
    await this.o.store.clear()
    if (rt) {
      await this.f(`${this.o.issuer}/protocol/openid-connect/logout`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ client_id: this.o.clientId, refresh_token: rt }),
      }).catch(() => {})
    }
  }
}
