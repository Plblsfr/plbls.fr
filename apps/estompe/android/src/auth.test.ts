import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const h = vi.hoisted(() => ({
  provider: null as null | (() => Promise<string | null> | string | null),
  opened: [] as string[],
  tokenCalls: [] as URLSearchParams[],
  tokenStatus: 200,
  fail: vi.fn(),
}))

vi.mock('../../frontend/src/lib/api', async (orig) => ({
  ...(await orig<object>()),
  setTokenProvider: (p: typeof h.provider) => { h.provider = p },
}))
vi.mock('../../frontend/src/lib/ui', async (orig) => {
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
  let refresh = ''
  return {
    deliver(url: string) { authUrl = url },
    takeAuthUrl: () => { const u = authUrl; authUrl = ''; return u },
    getRefreshToken: () => refresh,
    saveRefreshToken: (t: string) => { refresh = t },
    clearRefreshToken: () => { refresh = '' },
    configure: () => {},
    get refresh() { return refresh },
  }
}

async function freshAuth() {
  vi.resetModules()
  const { config } = await import('../../frontend/src/lib/config')
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
    const b1 = bridge()
    vi.stubGlobal('EstompeNative', b1)
    const first = await freshAuth()
    first.setup()
    void first.login(false).catch(() => {})
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    const state = stateOf(h.opened[0])

    const b2 = bridge(`fr.plbls.estompe:/callback?state=${state}&code=C1&iss=x`)
    vi.stubGlobal('EstompeNative', b2)
    const reloaded = await freshAuth()
    reloaded.setup()

    expect(await h.provider!()).toBe('A1')
    expect(h.tokenCalls).toHaveLength(1)
    expect(Object.fromEntries(h.tokenCalls[0])).toMatchObject({ grant_type: 'authorization_code', code: 'C1', client_id: 'plbls-android', redirect_uri: 'fr.plbls.estompe:/callback' })
    expect(b2.refresh).toBe('R1')
    expect(h.opened).toHaveLength(1)
  })

  it('ignore une redirection périmée gardée par le natif et attend celle de la nouvelle connexion', async () => {
    const b = bridge('fr.plbls.estompe:/callback?state=ancien&code=VIEUX')
    vi.stubGlobal('EstompeNative', b)
    const auth = await freshAuth()
    auth.setup()
    const login = auth.login(false)
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    b.deliver(`fr.plbls.estompe:/callback?state=${stateOf(h.opened[0])}&code=NEUF`)
    await vi.advanceTimersByTimeAsync(400)
    await login
    expect(h.tokenCalls.map((q) => q.get('code'))).toEqual(['NEUF'])
    expect(await h.provider!()).toBe('A1')
  })

  it('affiche l\'erreur si l\'échange repris échoue, sans rouvrir Keycloak', async () => {
    const b1 = bridge()
    vi.stubGlobal('EstompeNative', b1)
    const first = await freshAuth()
    first.setup()
    void first.login(false).catch(() => {})
    await vi.waitFor(() => expect(h.opened).toHaveLength(1))
    const state = stateOf(h.opened[0])

    h.tokenStatus = 400
    vi.stubGlobal('EstompeNative', bridge(`fr.plbls.estompe:/callback?state=${state}&code=C1`))
    const reloaded = await freshAuth()
    reloaded.setup()

    expect(await h.provider!()).toBeNull()
    expect(h.fail).toHaveBeenCalledWith(expect.stringContaining('Code not valid'))
    expect(h.opened).toHaveLength(1)
    expect(localStorage.length).toBe(0)
  })
})
