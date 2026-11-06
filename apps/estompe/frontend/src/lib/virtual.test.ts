import { describe, expect, it } from 'vitest'
import { windowOf } from './virtual'

const keys = Array.from({ length: 1000 }, (_, i) => String(i))
const h = () => 50

describe('windowOf', () => {
  it('ne garde que les lignes visibles et leur marge', () => {
    const w = windowOf(keys, h, 10_000, 600, 500)
    expect(w.start).toBe(190) // (10000 - 500) / 50
    expect(w.end).toBe(223)
    expect(w.top).toBe(190 * 50)
    expect(w.top + (w.end - w.start) * 50 + w.bottom).toBe(1000 * 50) // hauteur totale conservée
  })
  it('début de liste', () => {
    const w = windowOf(keys, h, 0, 600, 500)
    expect(w.start).toBe(0)
    expect(w.top).toBe(0)
    expect(w.end).toBeLessThan(30)
  })
  it('fin de liste', () => {
    const w = windowOf(keys, h, 1000 * 50 - 600, 600, 500)
    expect(w.end).toBe(1000)
    expect(w.bottom).toBe(0)
  })
  it('utilise les hauteurs mesurées', () => {
    const tall = (k: string) => (k === '0' ? 5000 : 50)
    const w = windowOf(keys, tall, 5200, 600, 100)
    expect(w.start).toBe(3) // la ligne 0 (5000 px) et deux lignes suivantes sont sous la marge
    expect(w.top).toBe(5000 + 2 * 50)
  })
  it('liste vide ou défilement au-delà de la fin', () => {
    expect(windowOf([], h, 0, 600)).toEqual({ start: 0, end: 0, top: 0, bottom: 0 })
    const w = windowOf(keys.slice(0, 3), h, 99_999, 600)
    expect(w.end).toBe(3)
    expect(w.start).toBe(2)
  })
})
