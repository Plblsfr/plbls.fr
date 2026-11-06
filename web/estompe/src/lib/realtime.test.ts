import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Realtime, wsUrl } from './realtime'

class FakeWS {
  static all: FakeWS[] = []
  readyState = 0
  sent: string[] = []
  onopen: any; onmessage: any; onclose: any; onerror: any
  constructor(public url: string) { FakeWS.all.push(this) }
  send(s: string) { this.sent.push(s) }
  close(code = 1000) { this.readyState = 3; this.onclose?.({ code }) }
  open() { this.readyState = 1; this.onopen?.() }
  recv(o: unknown) { this.onmessage?.({ data: JSON.stringify(o) }) }
  drop(code = 1006) { this.readyState = 3; this.onclose?.({ code }) }
}

function make(extra: Partial<ConstructorParameters<typeof Realtime>[0]> = {}) {
  const events: any[] = [], status: string[] = []
  let n = 0
  const rt = new Realtime({
    getTicket: async () => `t${++n}`,
    url: 'wss://chat.example/ws',
    onEvent: (e) => events.push(e),
    onStatus: (s) => status.push(s),
    WebSocketImpl: FakeWS as any,
    ...extra,
  })
  return { rt, events, status }
}
const flush = () => vi.advanceTimersByTimeAsync(0)

beforeEach(() => { vi.useFakeTimers(); FakeWS.all = []; vi.spyOn(Math, 'random').mockReturnValue(1) })
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks() })

describe('Realtime', () => {
  it("ajoute le ticket à une URL qui a déjà des paramètres", async () => {
    const { rt } = make({ url: 'wss://chat.example/ws?platform=web&v=1.0.0' })
    rt.start()
    await flush()
    expect(FakeWS.all[0].url).toBe('wss://chat.example/ws?platform=web&v=1.0.0&ticket=t1')
  })

  it('se connecte avec un ticket et transmet les évènements (sauf pong)', async () => {
    const { rt, events, status } = make()
    rt.start()
    await flush()
    expect(FakeWS.all[0].url).toBe('wss://chat.example/ws?ticket=t1')
    FakeWS.all[0].open()
    FakeWS.all[0].recv({ type: 'ready', data: {} })
    FakeWS.all[0].recv({ type: 'pong' })
    FakeWS.all[0].recv({ type: 'message.created', data: { id: '1' } })
    expect(events.map((e) => e.type)).toEqual(['ready', 'message.created'])
    expect(status).toEqual(['connecting', 'open'])
    rt.stop()
  })

  it('se reconnecte avec un nouveau ticket et signale la reconnexion', async () => {
    const onReconnected = vi.fn()
    const { rt } = make({ onReconnected })
    rt.start(); await flush()
    FakeWS.all[0].open()
    expect(onReconnected).not.toHaveBeenCalled() // première connexion
    FakeWS.all[0].drop()
    await vi.advanceTimersByTimeAsync(0) // premier essai immédiat
    expect(FakeWS.all).toHaveLength(2)
    expect(FakeWS.all[1].url).toContain('ticket=t2')
    FakeWS.all[1].open()
    expect(onReconnected).toHaveBeenCalledTimes(1)
    rt.stop()
  })

  it('réessaie aussitôt, puis attend de plus en plus longtemps, plafonné à 5 s', async () => {
    const { rt } = make()
    rt.start(); await flush()
    const waits: number[] = []
    for (let i = 0; i < 8; i++) {
      FakeWS.all.at(-1)!.drop()
      let t = 0
      const before = FakeWS.all.length
      while (FakeWS.all.length === before && t < 40_000) { await vi.advanceTimersByTimeAsync(500); t += 500 }
      waits.push(t)
    }
    expect(waits[0]).toBeLessThanOrEqual(500)
    expect(waits[3]).toBeGreaterThan(waits[1])
    expect(Math.max(...waits)).toBeLessThanOrEqual(5_500)
    rt.stop()
  })

  it('ne reboucle pas quand la session a expiré (4401)', async () => {
    const onSessionExpired = vi.fn()
    const { rt } = make({ onSessionExpired })
    rt.start(); await flush()
    FakeWS.all[0].open()
    FakeWS.all[0].drop(4401)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(onSessionExpired).toHaveBeenCalledTimes(1)
    expect(FakeWS.all).toHaveLength(1)
  })

  it("ne reboucle pas quand l'accès a été retiré (4403)", async () => {
    const onAccessRevoked = vi.fn()
    const { rt } = make({ onAccessRevoked })
    rt.start(); await flush()
    FakeWS.all[0].open()
    FakeWS.all[0].drop(4403)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(onAccessRevoked).toHaveBeenCalledTimes(1)
    expect(FakeWS.all).toHaveLength(1)
  })

  it('arrête tout si le ticket est refusé en 401', async () => {
    const onSessionExpired = vi.fn()
    const { rt } = make({ getTicket: async () => { throw { status: 401 } }, onSessionExpired })
    rt.start(); await flush()
    expect(onSessionExpired).toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(FakeWS.all).toHaveLength(0)
  })

  it('réessaie quand le ticket échoue pour une autre raison', async () => {
    let n = 0
    const { rt } = make({ getTicket: async () => { if (n++ === 0) throw new Error('réseau'); return 'ok' } })
    rt.start(); await flush()
    expect(FakeWS.all[0].url).toContain('ticket=ok')
    rt.stop()
  })

  it('envoie un ping régulier et coupe une connexion silencieuse', async () => {
    const { rt } = make()
    rt.start(); await flush()
    const ws = FakeWS.all[0]
    ws.open()
    await vi.advanceTimersByTimeAsync(10_000)
    expect(ws.sent).toContain('{"type":"ping"}')
    ws.recv({ type: 'pong' })
    await vi.advanceTimersByTimeAsync(20_000) // 30 s : dernier message il y a 20 s, la connexion est encore jugée vivante
    expect(FakeWS.all).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(10_000) // 40 s : silence de plus de 25 s → coupée puis reconnectée
    await vi.advanceTimersByTimeAsync(1)
    expect(FakeWS.all.length).toBeGreaterThanOrEqual(2)
    rt.stop()
  })

  it('retente aussitôt au retour du réseau au lieu d\'attendre', async () => {
    const { rt } = make()
    rt.start(); await flush()
    for (let i = 0; i < 4; i++) { FakeWS.all.at(-1)!.drop(); await vi.advanceTimersByTimeAsync(i === 0 ? 0 : 5000) }
    const n = FakeWS.all.length
    FakeWS.all.at(-1)!.drop()
    dispatchEvent(new Event('online'))
    await flush()
    expect(FakeWS.all).toHaveLength(n + 1)
    rt.stop()
  })

  it('stop() ferme et empêche toute reconnexion', async () => {
    const { rt, status } = make()
    rt.start(); await flush()
    FakeWS.all[0].open()
    rt.stop()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(FakeWS.all).toHaveLength(1)
    expect(status.at(-1)).toBe('closed')
  })
})

describe('wsUrl', () => {
  it('dérive l\'URL ws de l\'API', () => {
    expect(wsUrl('https://chat.plbls.fr')).toBe('wss://chat.plbls.fr/ws')
    expect(wsUrl('http://localhost:8080/')).toBe('ws://localhost:8080/ws')
    expect(wsUrl('', 'https://estompe.plbls.fr')).toBe('wss://estompe.plbls.fr/ws')
  })
})
