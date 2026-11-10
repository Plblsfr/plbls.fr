import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, ApiError, setClientPlatform, setTokenProvider, setUnauthorizedHandler, setUpgradeHandler } from './api'

const json = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status, headers: body === undefined ? {} : { 'Content-Type': 'application/json' } })

let fetchMock: ReturnType<typeof vi.fn>
beforeEach(() => { fetchMock = vi.fn(); vi.stubGlobal('fetch', fetchMock) })
afterEach(() => { vi.unstubAllGlobals(); setTokenProvider(null); setUnauthorizedHandler(null); setUpgradeHandler(null); setClientPlatform('') })

describe('api', () => {
  it('envoie les cookies et du JSON', async () => {
    fetchMock.mockResolvedValue(json(201, { id: 's1' }))
    const s = await api.createServer('Amis')
    expect(s).toEqual({ id: 's1' })
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/servers')
    expect(init.method).toBe('POST')
    expect(init.credentials).toBe('include')
    expect(init.headers['Content-Type']).toBe('application/json')
    expect(JSON.parse(init.body)).toEqual({ name: 'Amis' })
  })

  it('ajoute le Bearer fourni par l\'application Windows', async () => {
    setTokenProvider(async () => 'jeton')
    fetchMock.mockResolvedValue(json(200, { activated: true, allowed: true }))
    await api.me()
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe('Bearer jeton')
  })

  it('déclare sa version et sa plateforme, et signale le refus 426', async () => {
    setClientPlatform('web')
    const refus = vi.fn()
    setUpgradeHandler(refus)
    fetchMock.mockResolvedValue(json(426, { error: 'upgrade_required', message: 'faille corrigée' }))
    await expect(api.servers()).rejects.toMatchObject({ status: 426 })
    expect(fetchMock.mock.calls[0][1].headers).toMatchObject({ 'X-Estompe-Platform': 'web', 'X-Estompe-Version': __APP_VERSION__ })
    expect(refus).toHaveBeenCalledWith('faille corrigée')
  })

  it('n\'ajoute pas d\'en-tête Authorization sans jeton', async () => {
    fetchMock.mockResolvedValue(json(200, []))
    await api.servers()
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBeUndefined()
  })

  it('transforme les erreurs en ApiError avec le code du serveur', async () => {
    fetchMock.mockResolvedValue(json(409, { error: 'pseudo_taken', message: 'pseudo déjà pris' }))
    const err = await api.activate('bob').catch((e) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err).toMatchObject({ status: 409, code: 'pseudo_taken', message: 'pseudo déjà pris' })
  })

  it('signale les 401', async () => {
    const h = vi.fn()
    setUnauthorizedHandler(h)
    fetchMock.mockImplementation(async () => json(401, { error: 'unauthorized', message: 'x' }))
    await api.me().catch(() => {})
    expect(h).toHaveBeenCalledTimes(1)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('réessaie un 401 isolé sans signaler la perte de session', async () => {
    const h = vi.fn()
    setUnauthorizedHandler(h)
    fetchMock.mockImplementationOnce(async () => json(401, { error: 'unauthorized', message: 'x' }))
    fetchMock.mockImplementationOnce(async () => json(200, { allowed: true }))
    expect(await api.me()).toMatchObject({ allowed: true })
    expect(h).not.toHaveBeenCalled()
  })

  it('gère les 204 sans corps et les chemins encodés', async () => {
    fetchMock.mockImplementation(async () => json(204))
    expect(await api.react('12', '👍')).toBeUndefined()
    expect(fetchMock.mock.calls[0][0]).toBe('/messages/12/reactions/' + encodeURIComponent('👍'))
    await api.revokeServerInvite('s1', 'AB/CD')
    expect(fetchMock.mock.calls[1][0]).toBe('/servers/s1/invites/AB%2FCD')
  })

  it('envoie un fichier en multipart sans forcer le Content-Type', async () => {
    fetchMock.mockResolvedValue(json(201, { id: 'f1' }))
    await api.upload(new Blob(['x']), 'a.txt')
    const init = fetchMock.mock.calls[0][1]
    expect(init.body).toBeInstanceOf(FormData)
    expect(init.headers['Content-Type']).toBeUndefined()
  })

  it('construit la pagination', async () => {
    fetchMock.mockImplementation(async () => json(200, { messages: [], has_more: false }))
    await api.channelMessages('c1', '42', 30)
    expect(fetchMock.mock.calls[0][0]).toBe('/channels/c1/messages?limit=30&before=42')
    await api.dmMessages('d1')
    expect(fetchMock.mock.calls[1][0]).toBe('/dms/d1/messages?limit=50')
  })

  it("construit les routes des badges (pseudo encodé)", async () => {
    fetchMock.mockImplementation(async () => json(204))
    await api.grantBadge('b1', 'a/b c')
    await api.revokeBadge('b1', 'x')
    expect(fetchMock.mock.calls[0].slice(0, 2).map((x) => (typeof x === 'string' ? x : x.method))).toEqual(['/admin/badges/b1/users/a%2Fb%20c', 'PUT'])
    expect(fetchMock.mock.calls[1][1].method).toBe('DELETE')
  })

  it("construit les routes de modification des badges", async () => {
    fetchMock.mockImplementation(async () => json(204))
    await api.updateBadge('b1', { name: 'N', color: '#000000', description: '', sort_order: 1 })
    await api.grantBadges('b1', ['a', 'b'])
    await api.deleteBadgeIcon('b1')
    const calls = fetchMock.mock.calls.map((c) => [c[0], c[1].method])
    expect(calls).toEqual([['/admin/badges/b1', 'PUT'], ['/admin/badges/b1/users', 'POST'], ['/admin/badges/b1/icon', 'DELETE']])
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({ pseudos: ['a', 'b'] })
  })

  it("construit les routes de l'espace d'administration", async () => {
    fetchMock.mockImplementation(async () => json(204))
    await api.adminUsers('a b')
    await api.suspendUser('u1', true)
    await api.suspendUser('u1', false)
    await api.adminDeleteServer('s1')
    const calls = fetchMock.mock.calls.map((c) => [c[0], c[1].method])
    expect(calls).toEqual([
      ['/admin/users?q=a%20b', 'GET'],
      ['/admin/users/u1/suspension', 'PUT'],
      ['/admin/users/u1/suspension', 'DELETE'],
      ['/admin/servers/s1', 'DELETE'],
    ])
  })
  it("envoie les images par le même chemin que les autres requêtes (401 réessayé puis signalé, 426)", async () => {
    const h = vi.fn()
    const refus = vi.fn()
    setUnauthorizedHandler(h)
    setUpgradeHandler(refus)
    const img = new Blob(['x'], { type: 'image/png' })
    fetchMock.mockImplementationOnce(async () => json(401, { error: 'unauthorized', message: 'x' }))
    fetchMock.mockImplementationOnce(async () => json(204))
    await api.setAvatar(img)
    const [url, init] = fetchMock.mock.calls[1]
    expect([url, init.method, init.body, init.headers['Content-Type']]).toEqual(['/me/avatar', 'PUT', img, 'image/png'])
    expect(h).not.toHaveBeenCalled()
    fetchMock.mockImplementation(async () => json(401, { error: 'unauthorized', message: 'x' }))
    await expect(api.setServerAvatar('s1', img)).rejects.toMatchObject({ status: 401 })
    await expect(api.avatarBlob('u1', 's1')).rejects.toMatchObject({ status: 401 })
    expect(h).toHaveBeenCalledTimes(2)
    fetchMock.mockImplementation(async () => json(426, { error: 'upgrade_required', message: 'trop vieux' }))
    await expect(api.setCameraBackground(img)).rejects.toMatchObject({ status: 426 })
    await expect(api.serverIconBlobUrl('s1')).rejects.toMatchObject({ status: 426 })
    expect(refus).toHaveBeenCalledTimes(2)
  })

  it('borne chaque requête dans le temps et signale un serveur muet', async () => {
    fetchMock.mockResolvedValue(json(200, []))
    await api.servers()
    expect(fetchMock.mock.calls[0][1].signal).toBeInstanceOf(AbortSignal)
    fetchMock.mockImplementation(async () => { throw new DOMException('délai dépassé', 'TimeoutError') })
    await expect(api.servers()).rejects.toMatchObject({ status: 0, code: 'timeout' })
    await expect(api.avatarBlob('u1')).rejects.toMatchObject({ status: 0, code: 'timeout' })
  })
})
