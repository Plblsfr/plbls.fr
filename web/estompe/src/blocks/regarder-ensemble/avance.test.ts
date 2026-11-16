import { afterEach, describe, expect, it, vi } from 'vitest'
import { Avance, BUDGETS, type Frag } from './avance'

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

  it('charge dans l’ordre depuis la position, un segment à la fois, jusqu’à la durée prévue', async () => {
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

  it('ne garde rien après destruction', async () => {
    const a = new Avance({ seconds: 100, bytes: 1e9 }, () => 0, async () => blob(10))
    a.setFragments(frags(3))
    await settle()
    a.destroy()
    expect(a.ranges()).toEqual([])
    expect(a.bytes).toBe(0)
  })
})
