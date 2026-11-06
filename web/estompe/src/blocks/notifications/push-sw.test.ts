import { describe, it, expect, vi } from 'vitest'
import source from '../../../public/push-sw.js?raw'

/** Charge public/push-sw.js dans un faux contexte de service worker et renvoie ses gestionnaires. */
function load() {
  const handlers: Record<string, (e: unknown) => void> = {}
  const shown: unknown[] = []
  const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
  const self = {
    __ESTOMPE__: { apiUrl: 'https://api.test' },
    addEventListener: (t: string, h: (e: unknown) => void) => { handlers[t] = h },
    clients: { matchAll: () => Promise.resolve([]), openWindow: vi.fn(() => Promise.resolve()) },
    registration: { showNotification: (...a: unknown[]) => { shown.push(a); return Promise.resolve() } },
  }
  new Function('self', 'importScripts', 'fetch', source)(self, () => {}, fetchMock)
  return { handlers, shown, fetchMock, self }
}

const run = async (h: (e: unknown) => void, e: Record<string, unknown>) => {
  let p: Promise<unknown> = Promise.resolve()
  h({ waitUntil: (x: Promise<unknown>) => { p = x }, notification: { close() {}, data: {} }, ...e })
  await p
}

describe('service worker push', () => {
  it('affiche les actions rapides pour un message, pas pour le vocal', async () => {
    const { handlers, shown } = load()
    const data = (o: object) => ({ json: () => o })
    await run(handlers.push, { data: data({ title: 'a', conv: 'c:1', msg: '9', url: '/#/s/1/1' }) })
    await run(handlers.push, { data: data({ title: 'Estompe', conv: 'c:1' }) })
    const [first, second] = shown as [string, { actions: { action: string }[]; data: { url: string } }][]
    expect(first[1].actions.map((a) => a.action)).toEqual(['reply', 'read'])
    expect(first[1].data.url).toBe('/#/s/1/1')
    expect(second[1].actions).toEqual([])
  })

  it('affiche la notification même si l\'application est au premier plan (Safari révoque sinon l\'abonnement)', async () => {
    const { handlers, shown, self } = load()
    self.clients.matchAll = () => Promise.resolve([{ visibilityState: 'visible', focused: true }] as never[])
    await run(handlers.push, { data: { json: () => ({ title: 'test' }) } })
    expect(shown).toHaveLength(1)
  })

  it('« marquer comme lu » appelle le bon endpoint', async () => {
    const { handlers, fetchMock } = load()
    await run(handlers.notificationclick, { action: 'read', notification: { close() {}, data: { conv: 'd:7', msg: '1' } } })
    expect(fetchMock).toHaveBeenCalledWith('https://api.test/dms/7/read', expect.objectContaining({ method: 'POST', credentials: 'include' }))
  })

  it('« répondre » envoie la réponse au message ; sans texte, ouvre la conversation', async () => {
    const { handlers, fetchMock, self } = load()
    const n = { close() {}, data: { conv: 'c:3', msg: '42', url: '/#/s/1/3' } }
    await run(handlers.notificationclick, { action: 'reply', reply: ' ok ', notification: n })
    expect(fetchMock).toHaveBeenCalledWith('https://api.test/channels/3/messages', expect.objectContaining({ body: JSON.stringify({ content: 'ok', reply_to: '42' }) }))
    await run(handlers.notificationclick, { action: 'reply', notification: n })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(self.clients.openWindow).toHaveBeenCalledWith('/#/s/1/3')
  })
})
