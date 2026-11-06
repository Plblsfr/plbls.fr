import { describe, expect, it, vi } from 'vitest'
import { authUrl, challengeS256, localSecretStore, TokenManager, type SecretStore } from './desktop-auth'

const ISS = 'https://auth.example/realms/plbls'

function memStore(initial: string | null = null): SecretStore & { value: string | null } {
  const s = { value: initial, load: async () => s.value, save: async (v: string) => { s.value = v }, clear: async () => { s.value = null } }
  return s
}
const ok = (o: object) => new Response(JSON.stringify(o), { status: 200, headers: { 'Content-Type': 'application/json' } })
const bad = (status: number, error = 'invalid_grant') => new Response(JSON.stringify({ error }), { status })

describe('PKCE', () => {
  it('calcule le défi S256 de l\'exemple de la RFC 7636', async () => {
    expect(await challengeS256('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk')).toBe('E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM')
  })
  it('construit l\'URL d\'autorisation (connexion ou inscription)', () => {
    const base = { issuer: ISS, clientId: 'plbls-desktop', redirectUri: 'http://127.0.0.1:5555/callback', state: 'st', challenge: 'ch' }
    const u = new URL(authUrl(base))
    expect(u.pathname).toBe('/realms/plbls/protocol/openid-connect/auth')
    expect(Object.fromEntries(u.searchParams)).toMatchObject({
      client_id: 'plbls-desktop', response_type: 'code', code_challenge: 'ch', code_challenge_method: 'S256', redirect_uri: 'http://127.0.0.1:5555/callback', state: 'st',
    })
    expect(new URL(authUrl({ ...base, register: true })).pathname).toMatch(/\/registrations$/)
  })
})

describe('TokenManager', () => {
  it('échange le code puis garde seulement le refresh token dans le coffre', async () => {
    const store = memStore()
    const f = vi.fn(async () => ok({ access_token: 'A1', refresh_token: 'R1', expires_in: 300 }))
    const tm = new TokenManager({ issuer: ISS, clientId: 'plbls-desktop', store, fetch: f as any })
    await tm.loginWithCode('code', 'verif', 'http://127.0.0.1:1/callback')
    const body = (f.mock.calls[0] as any)[1].body as URLSearchParams
    expect(Object.fromEntries(body)).toMatchObject({ grant_type: 'authorization_code', code: 'code', code_verifier: 'verif', client_id: 'plbls-desktop' })
    expect(store.value).toBe('R1')
    expect(await tm.getAccessToken()).toBe('A1')
    expect(f).toHaveBeenCalledTimes(1) // encore valide : pas de renouvellement
  })

  it('renouvelle près de l\'expiration et remplace le refresh token (rotation)', async () => {
    let t = 1_000_000
    const store = memStore('R1')
    const f = vi.fn(async (_u: string, init: any) => {
      const rt = init.body.get('refresh_token')
      return rt === 'R1' ? ok({ access_token: 'A2', refresh_token: 'R2', expires_in: 300 }) : bad(400)
    })
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: f as any, now: () => t })
    expect(await tm.getAccessToken()).toBe('A2')
    expect(store.value).toBe('R2')
    t += 280_000 // 20 s avant l'expiration : sous la marge de 30 s
    f.mockImplementation(async () => ok({ access_token: 'A3', refresh_token: 'R3', expires_in: 300 }))
    expect(await tm.getAccessToken()).toBe('A3')
    expect(store.value).toBe('R3')
  })

  it('regroupe les renouvellements simultanés en un seul appel', async () => {
    const store = memStore('R1')
    let calls = 0
    const f = vi.fn(async () => { calls++; await new Promise((r) => setTimeout(r, 10)); return ok({ access_token: 'A', refresh_token: 'R2', expires_in: 300 }) })
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: f as any })
    const [a, b, c] = await Promise.all([tm.getAccessToken(), tm.getAccessToken(), tm.getAccessToken()])
    expect([a, b, c]).toEqual(['A', 'A', 'A'])
    expect(calls).toBe(1)
  })

  it('oublie la session quand Keycloak refuse le refresh token', async () => {
    const store = memStore('R-perime')
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: (async () => bad(400)) as any })
    expect(await tm.getAccessToken()).toBeNull()
    expect(store.value).toBeNull()
    expect(await tm.hasSession()).toBe(false)
  })

  it('garde le refresh token quand Keycloak est injoignable', async () => {
    const store = memStore('R1')
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: (async () => { throw new TypeError('réseau') }) as any })
    await expect(tm.getAccessToken()).rejects.toThrow()
    expect(store.value).toBe('R1')
    const tm503 = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: (async () => bad(503, 'x')) as any })
    await expect(tm503.getAccessToken()).rejects.toThrow()
    expect(store.value).toBe('R1')
  })

  it('sans refresh token, pas de session', async () => {
    const f = vi.fn()
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store: memStore(), fetch: f as any })
    expect(await tm.getAccessToken()).toBeNull()
    expect(f).not.toHaveBeenCalled()
  })

  it('se déconnecte : révoque côté Keycloak puis efface tout', async () => {
    const store = memStore('R1')
    const f = vi.fn(async () => new Response(null, { status: 204 }))
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: f as any })
    await tm.logout()
    expect((f.mock.calls[0] as any)[0]).toBe(`${ISS}/protocol/openid-connect/logout`)
    expect(((f.mock.calls[0] as any)[1].body as URLSearchParams).get('refresh_token')).toBe('R1')
    expect(store.value).toBeNull()
    expect(await tm.getAccessToken()).toBeNull()
  })

  it('se déconnecte même si Keycloak ne répond pas', async () => {
    const store = memStore('R1')
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store, fetch: (async () => { throw new Error('hors ligne') }) as any })
    await tm.logout()
    expect(store.value).toBeNull()
  })
})

describe('reprise de session Android', () => {
  it('un nouveau démarrage retrouve la session depuis le stockage local, et la déconnexion l\'efface', async () => {
    const data = new Map<string, string>()
    const ls = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), removeItem: (k: string) => void data.delete(k) }
    const f = async () => ok({ access_token: 'A', refresh_token: 'R2', expires_in: 300 })
    await localSecretStore('rt', ls).save('R1')
    const tm = new TokenManager({ issuer: ISS, clientId: 'c', store: localSecretStore('rt', ls), fetch: f as any }) // « redémarrage » : instance neuve
    expect(await tm.hasSession()).toBe(true)
    expect(data.get('rt')).toBe('R2')
    await tm.logout()
    expect(data.has('rt')).toBe(false)
  })
})
