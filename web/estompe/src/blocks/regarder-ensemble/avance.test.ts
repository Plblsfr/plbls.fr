import { afterEach, describe, expect, it, vi } from 'vitest'
import { Avance, BUDGETS, PARALLELE, PasPret, nomDeSegment, type Frag } from './avance'
import { memoire } from './depot'

const frags = (n: number, d = 4): Frag[] => Array.from({ length: n }, (_, i) => ({ url: `seg${i}`, start: i * d, duration: d }))
const blob = (size: number) => new Blob([new Uint8Array(size)])
const settle = async () => { for (let i = 0; i < 50; i++) await Promise.resolve() }

describe('préchargement des segments d’un film', () => {
  afterEach(() => vi.useRealTimers())

  it('ne précharge rien de plus au réglage normal, et prévoit plus au réglage long qu’au réglage complet borné', () => {
    expect(BUDGETS.normal).toBeNull()
    expect(BUDGETS.long!.seconds).toBe(900)
    expect(BUDGETS.all!.seconds).toBe(Number.POSITIVE_INFINITY)
    expect(BUDGETS.all!.bytes).toBeGreaterThan(BUDGETS.long!.bytes)
  })

  it('charge dans l’ordre depuis la position jusqu’à la durée prévue', async () => {
    const asked: string[] = []
    const a = new Avance({ seconds: 10, bytes: 1e9 }, () => 9, async (u) => { asked.push(u); return blob(10) })
    a.setFragments(frags(10))
    await settle()
    expect(asked).toEqual(['seg2', 'seg3', 'seg4'])
    expect(a.ranges()).toEqual([[8, 20]])
    expect(a.bytes).toBe(30)
  })

  it('charge tout le film quand la durée n’est pas bornée, et suit la liste qui s’allonge', async () => {
    const asked: string[] = []
    const a = new Avance({ seconds: Number.POSITIVE_INFINITY, bytes: 1e9 }, () => 0, async (u) => { asked.push(u); return blob(10) })
    a.setFragments(frags(3))
    await settle()
    expect(a.ranges()).toEqual([[0, 12]])
    a.setFragments(frags(6))
    await settle()
    expect(asked).toHaveLength(6)
    expect(a.ranges()).toEqual([[0, 24]])
  })

  it('repart de la nouvelle position après un saut et montre les deux plages', async () => {
    let pos = 0
    const a = new Avance({ seconds: 8, bytes: 1e9 }, () => pos, async () => blob(10))
    a.setFragments(frags(20))
    await settle()
    expect(a.ranges()).toEqual([[0, 12]])
    pos = 40
    a.wake()
    await settle()
    expect(a.ranges()).toEqual([[0, 12], [40, 52]])
  })

  it('ne télécharge un segment qu’une fois, qu’il soit demandé par le lecteur ou par l’avance', async () => {
    const get = vi.fn(async () => blob(10))
    const a = new Avance({ seconds: 4, bytes: 1e9 }, () => 0, get)
    const p = a.fetch('seg0')
    a.setFragments(frags(1))
    expect((await p).size).toBe(10)
    await settle()
    await a.fetch('seg0')
    expect(get).toHaveBeenCalledTimes(1)
  })

  it('libère ce qui est loin derrière la position quand la mémoire prévue est pleine, sinon s’arrête', async () => {
    let pos = 0
    const a = new Avance({ seconds: Number.POSITIVE_INFINITY, bytes: 50 }, () => pos, async () => blob(10))
    a.setFragments(frags(30))
    await settle()
    expect(a.ranges()).toEqual([[0, 20]])
    pos = 60
    a.wake()
    await settle()
    expect(a.bytes).toBeLessThanOrEqual(50)
    expect(a.ranges()).toEqual([[60, 80]])
  })

  it('réessaie après un échec puis abandonne au bout de trois', async () => {
    vi.useFakeTimers()
    const get = vi.fn(async () => { throw new Error('réseau') })
    const a = new Avance({ seconds: 4, bytes: 1e9 }, () => 0, get)
    a.setFragments(frags(2))
    for (let i = 0; i < 6; i++) { await settle(); await vi.advanceTimersByTimeAsync(5000) }
    expect(get).toHaveBeenCalledTimes(3)
    expect(a.ranges()).toEqual([])
  })

  it('télécharge plusieurs segments à la fois après le premier, jamais plus que prévu, le plus proche d’abord', async () => {
    const asked: string[] = []
    const pending: (() => void)[] = []
    let flying = 0
    let most = 0
    const a = new Avance({ seconds: Number.POSITIVE_INFINITY, bytes: 1e9 }, () => 9, (u, d) => new Promise((ok) => {
      expect(d.prefetch).toBe(true)
      asked.push(u)
      most = Math.max(most, ++flying)
      pending.push(() => { flying--; ok(blob(10)) })
    }))
    a.setFragments(frags(12))
    await settle()
    expect(asked).toEqual(['seg2'])
    pending.shift()!()
    await settle()
    expect(asked).toEqual(['seg2', 'seg3', 'seg4', 'seg5'])
    a.wake()
    await settle()
    expect(asked).toHaveLength(4)
    while (pending.length) { pending.shift()!(); await settle() }
    expect(asked).toEqual([2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((i) => `seg${i}`))
    expect(most).toBe(PARALLELE)
    expect(a.ranges()).toEqual([[8, 48]])
  })

  it('suit un réglage qui change : rien au réglage normal, reprise dès qu’il le permet', async () => {
    let budget: { seconds: number; bytes: number } | null = null
    const get = vi.fn(async () => blob(10))
    const a = new Avance(() => budget, () => 0, get)
    a.setFragments(frags(3))
    await settle()
    expect(get).not.toHaveBeenCalled()
    expect(a.active).toBe(false)
    budget = { seconds: 100, bytes: 1e9 }
    a.wake()
    await settle()
    expect(get).toHaveBeenCalledTimes(3)
    budget = null
    a.setFragments(frags(6))
    await settle()
    expect(get).toHaveBeenCalledTimes(3)
    expect(a.has('seg1')).toBe(true)
  })

  it('attend un segment que le serveur n’a pas encore, sans compter d’échec', async () => {
    vi.useFakeTimers()
    let ready = false
    let pos = 0
    const get = vi.fn(async (u: string) => { if (u === 'seg1' && !ready) throw new PasPret(); return blob(10) })
    const a = new Avance({ seconds: 8, bytes: 1e9 }, () => pos, get)
    a.setFragments(frags(10))
    await settle()
    expect(get.mock.calls.map((c) => c[0])).toEqual(['seg0', 'seg1', 'seg2'])
    for (let i = 0; i < 5; i++) { a.wake(); await settle() }
    expect(get).toHaveBeenCalledTimes(3)
    pos = 20
    a.wake()
    await settle()
    expect(a.ranges()).toEqual([[0, 4], [8, 12], [20, 32]])
    pos = 0
    ready = true
    for (let i = 0; i < 4; i++) { await vi.advanceTimersByTimeAsync(5000); await settle() }
    expect(a.ranges()).toEqual([[0, 12], [20, 32]])
  })

  it('reconnaît un segment par son nom quand le flux est rouvert, et ne le redemande pas', async () => {
    expect(nomDeSegment('/stream/jeton.1.x/seg00012.ts?avance=1')).toBe('seg00012.ts')
    const depot = memoire()
    const jeton = (j: string, n: number): Frag[] => frags(n).map((f, i) => ({ ...f, url: `/stream/${j}/seg${i}.ts` }))
    const premier = vi.fn(async () => blob(10))
    const a = new Avance({ seconds: 100, bytes: 1e9 }, () => 0, premier, { depot })
    a.setFragments(jeton('a', 3))
    await settle()
    a.destroy()
    expect(depot.bytes()).toBe(30)
    const second = vi.fn(async (_url: string) => blob(10))
    const b = new Avance({ seconds: 100, bytes: 1e9 }, () => 0, second, { depot })
    b.setFragments(jeton('b', 4))
    await settle()
    expect(second.mock.calls.map((c) => c[0])).toEqual(['/stream/b/seg3.ts'])
    expect(b.ranges()).toEqual([[0, 16]])
    expect((await b.fetch('/stream/b/seg1.ts')).size).toBe(10)
    expect(second).toHaveBeenCalledTimes(1)
  })

  it('libère d’abord les segments qui ne sont plus dans la liste du flux', async () => {
    const depot = memoire()
    await depot.put('ancien0.ts', blob(20))
    await depot.put('ancien1.ts', blob(20))
    const a = new Avance({ seconds: 100, bytes: 50 }, () => 0, async () => blob(10), { depot })
    a.setFragments(frags(4))
    await settle()
    expect(depot.names().filter((n) => n.startsWith('ancien'))).toEqual([])
    expect(a.ranges()).toEqual([[0, 16]])
    expect(a.bytes).toBeLessThanOrEqual(50)
  })

  it('redemande pour la lecture un segment que le préchargement n’a pas obtenu', async () => {
    const seen: boolean[] = []
    const a = new Avance({ seconds: 4, bytes: 1e9 }, () => 0, async (_u, d) => { seen.push(d.prefetch); if (d.prefetch) throw new PasPret(); return blob(7) })
    a.setFragments(frags(1))
    expect((await a.fetch('seg0')).size).toBe(7)
    expect(seen).toEqual([true, false])
    a.destroy()
  })

  it('ne garde rien après destruction', async () => {
    const a = new Avance({ seconds: 100, bytes: 1e9 }, () => 0, async () => blob(10))
    a.setFragments(frags(3))
    await settle()
    a.destroy()
    expect(a.ranges()).toEqual([])
    expect(a.bytes).toBe(0)
  })
})
