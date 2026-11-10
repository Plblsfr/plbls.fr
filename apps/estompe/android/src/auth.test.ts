import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const h = vi.hoisted(() => ({
  provider: null as null | (() => Promise<string | null> | string | null),
  opened: [] as string[],
  tokenCalls: [] as URLSearchParams[],
  tokenStatus: 200,
  fail: vi.fn(),
}))

vi.mock('../../../../web/estompe/src/lib/api', async (orig) => ({
  ...(await orig<object>()),
  setTokenProvider: (p: typeof h.provider) => { h.provider = p },
}))
vi.mock('../../../../web/estompe/src/lib/ui', async (orig) => {
  const m = await orig<{ notify: object }>()
  return { ...m, notify: { ...m.notify, fail: h.fail } }
})
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: async (u: string) => { h.opened.push(u) } }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args: { body: string | null }) => {
    if (cmd !== 'native_fetch') throw new Error(cmd)
    h.tokenCalls.push(new URLSearchParams(args.body ?? ''))
    return h.tokenStatus === 200
      ? { status: 200, body: JSON.stringify({ access_token: 'A1', refresh_token: 'R1', expires_in: 300 }) }
      : { status: h.tokenStatus, body: JSON.stringify({ error: 'invalid_grant', error_description: 'Code not valid' }) }
  },
}))

function bridge(pending = '') {
  let authUrl = pending
  return {
    deliver(url: string) { authUrl = url },
    takeAuthUrl: () => { const u = authUrl; authUrl = ''; return u },
  }
}

function session(o: { refresh?: string; loginStatus?: number; renewStatus?: number } = {}) {
  let refresh = o.refresh ?? ''
  const requests: Record<string, string>[] = []
  const replies: Record<string, unknown>[] = []
  const channel = {
    onmessage: null as null | ((e: { data: string }) => void),
    postMessage(m: string) {
      const r = JSON.parse(m)
      requests.push(r)
      let reply: Record<string, unknown>
      if (r.op === 'login') {
        const status = o.loginStatus ?? 200
        if (status === 200) refresh = 'R1'
        reply = status === 200 ? { status, access_token: 'A1', expires_in: 300 } : { status, error: 'Code not valid' }
      } else if (r.op === 'renew') {
        const status = refresh ? o.renewStatus ?? 200 : 401
        if (status === 400 || status === 401) refresh = ''
        reply = status === 200 ? { status, access_token: 'A2', expires_in: 300 } : { status }
      } else if (r.op === 'adopt') {
        if (!refresh) refresh = r.refresh
        reply = { status: 200 }
      } else if (r.op === 'logout') {
        refresh = ''
        reply = { status: 200 }
      } else reply = { status: 400 }
      replies.push(reply)
      queueMicrotask(() => channel.onmessage?.({ data: JSON.stringify({ ...reply, id: r.id }) }))
    },
  }
  return { channel, requests, replies, get refresh() { return refresh } }
}

async function freshAuth() {
  vi.resetModules()
  const { config } = await import('../../../../web/estompe/src/lib/config')
  config.keycloakIssuer = 'https://auth.example/realms/default'
  return (await import('./auth')).auth
}

const stateOf = (url: string) => new URL(url).searchParams.get('state')!

describe('connexion Android', () => {
  beforeEach(() => {
    h.provider = null
    h.opened = []
    h.tokenCalls = []
    h.tokenStatus = 200
    h.fail.mockReset()
    localStorage.clear()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('échange le code reçu après que la page de l\'app a été rechargée pendant la connexion', async () => {
    const s = session()
    vi.stubGlobal('EstompeSession', s.channel)
    vi.stubGlobal('EstompeNative', bridge())
    const first = await freshAuth()
    first.setup()
    void first.login(false).catch(() => {})
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    const state = stateOf(h.opened[0])

    vi.stubGlobal('EstompeNative', bridge(`fr.plbls.estompe:/callback?state=${state}&code=C1&iss=x`))
    const reloaded = await freshAuth()
    reloaded.setup()

    expect(await h.provider!()).toBe('A1')
    expect(h.tokenCalls).toHaveLength(0)
    const logins = s.requests.filter((r) => r.op === 'login')
    expect(logins).toHaveLength(1)
    expect(logins[0]).toMatchObject({ code: 'C1', redirectUri: 'fr.plbls.estompe:/callback' })
    expect(logins[0].verifier).toBeTruthy()
    expect(s.refresh).toBe('R1')
    expect(h.opened).toHaveLength(1)
  })

  it('renouvelle par le natif sans jamais recevoir le refresh token', async () => {
    const s = session({ refresh: 'R0' })
    vi.stubGlobal('EstompeSession', s.channel)
    vi.stubGlobal('EstompeNative', bridge())
    const auth = await freshAuth()
    auth.setup()

    expect(await h.provider!()).toBe('A2')
    expect(s.requests.map((r) => r.op)).toEqual(['renew'])
    expect(JSON.stringify(s.replies)).not.toContain('R0')
    expect(h.tokenCalls).toHaveLength(0)
    expect(localStorage.length).toBe(0)
  })

  it('session refusée par Keycloak : aucun jeton, sans lever', async () => {
    const s = session({ refresh: 'R0', renewStatus: 400 })
    vi.stubGlobal('EstompeSession', s.channel)
    vi.stubGlobal('EstompeNative', bridge())
    const auth = await freshAuth()
    auth.setup()
    expect(await h.provider!()).toBeNull()
  })

  it('panne passagère du renouvellement : lève au lieu de renvoyer une absence de session', async () => {
    const s = session({ refresh: 'R0', renewStatus: 503 })
    vi.stubGlobal('EstompeSession', s.channel)
    vi.stubGlobal('EstompeNative', bridge())
    const auth = await freshAuth()
    auth.setup()
    await expect(h.provider!()).rejects.toThrow('HTTP 503')
  })

  it('confie au natif le refresh token d\'une ancienne version gardé par le front, puis l\'efface', async () => {
    localStorage.setItem('refresh-token', 'ANCIEN')
    const s = session()
    vi.stubGlobal('EstompeSession', s.channel)
    vi.stubGlobal('EstompeNative', bridge())
    const auth = await freshAuth()
    auth.setup()
    expect(await h.provider!()).toBe('A2')
    expect(s.requests.map((r) => r.op)).toEqual(['adopt', 'renew'])
    expect(s.refresh).toBe('ANCIEN')
    expect(localStorage.getItem('refresh-token')).toBeNull()
  })

  it('déconnexion : le natif oublie et révoque la session', async () => {
    const s = session({ refresh: 'R0' })
    vi.stubGlobal('EstompeSession', s.channel)
    vi.stubGlobal('EstompeNative', bridge())
    const auth = await freshAuth()
    auth.setup()
    expect(await h.provider!()).toBe('A2')
    await auth.logout()
    expect(s.refresh).toBe('')
    expect(await h.provider!()).toBeNull()
  })

  it('sans canal de session (WebView trop ancienne) : refresh token dans le stockage de l\'app, échange par native_fetch', async () => {
    const b1 = bridge()
    vi.stubGlobal('EstompeNative', b1)
    const first = await freshAuth()
    first.setup()
    void first.login(false).catch(() => {})
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    const state = stateOf(h.opened[0])

    vi.stubGlobal('EstompeNative', bridge(`fr.plbls.estompe:/callback?state=${state}&code=C1`))
    const reloaded = await freshAuth()
    reloaded.setup()

    expect(await h.provider!()).toBe('A1')
    expect(Object.fromEntries(h.tokenCalls[0])).toMatchObject({ grant_type: 'authorization_code', code: 'C1', client_id: 'plbls-android' })
    expect(localStorage.getItem('refresh-token')).toBe('R1')
  })

  it('ignore une redirection périmée gardée par le natif et attend celle de la nouvelle connexion', async () => {
    const s = session()
    vi.stubGlobal('EstompeSession', s.channel)
    const b = bridge('fr.plbls.estompe:/callback?state=ancien&code=VIEUX')
    vi.stubGlobal('EstompeNative', b)
    const auth = await freshAuth()
    auth.setup()
    const login = auth.login(false)
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    b.deliver(`fr.plbls.estompe:/callback?state=${stateOf(h.opened[0])}&code=NEUF`)
    await vi.advanceTimersByTimeAsync(400)
    await login
    expect(s.requests.filter((r) => r.op === 'login').map((r) => r.code)).toEqual(['NEUF'])
    expect(await h.provider!()).toBe('A1')
  })

  it('affiche l\'erreur si l\'échange repris échoue, sans rouvrir Keycloak', async () => {
    vi.stubGlobal('EstompeSession', session({ loginStatus: 400 }).channel)
    const b1 = bridge()
    vi.stubGlobal('EstompeNative', b1)
    const first = await freshAuth()
    first.setup()
    void first.login(false).catch(() => {})
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    const state = stateOf(h.opened[0])

    vi.stubGlobal('EstompeNative', bridge(`fr.plbls.estompe:/callback?state=${state}&code=C1`))
    const reloaded = await freshAuth()
    reloaded.setup()

    expect(await h.provider!()).toBeNull()
    expect(h.fail).toHaveBeenCalledWith(expect.stringContaining('Code not valid'))
    expect(h.opened).toHaveLength(1)
    expect(localStorage.length).toBe(0)
  })
})
