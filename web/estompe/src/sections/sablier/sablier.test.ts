import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { setUnauthorizedHandler } from '../../lib/api'
import { sablierApi } from './sablier'

const json = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status, headers: body === undefined ? {} : { 'Content-Type': 'application/json' } })

let fetchMock: ReturnType<typeof vi.fn>
beforeEach(() => { fetchMock = vi.fn(); vi.stubGlobal('fetch', fetchMock) })
afterEach(() => { vi.unstubAllGlobals(); setUnauthorizedHandler(null) })

describe('client de Sablier', () => {
  it('envoie le battement avec la session, la plateforme et la section', async () => {
    fetchMock.mockResolvedValue(json(204))
    await sablierApi.battre('pwa', 'sillon')
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/heartbeat')
    expect(init.method).toBe('POST')
    expect(init.credentials).toBe('include')
    expect(JSON.parse(init.body)).toEqual({ platform: 'pwa', section: 'sillon' })
  })

  it('lit la période annoncée par /me', async () => {
    fetchMock.mockResolvedValue(json(200, { sub: 'u', heartbeat_seconds: 30 }))
    expect((await sablierApi.me()).heartbeat_seconds).toBe(30)
    expect(fetchMock.mock.calls[0][0]).toBe('/me')
  })

  it('un 401 ne déconnecte pas et n\'est pas retenté', async () => {
    const perdue = vi.fn()
    setUnauthorizedHandler(perdue)
    fetchMock.mockImplementation(async () => json(401, { error: 'unauthorized' }))
    await expect(sablierApi.battre('web', 'messagerie')).rejects.toMatchObject({ status: 401 })
    await expect(sablierApi.me()).rejects.toMatchObject({ status: 401 })
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(perdue).not.toHaveBeenCalled()
  })
})
