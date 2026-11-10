import { describe, expect, it, vi } from 'vitest'
import { Lru } from './lru'

describe('Lru', () => {
  it('oublie la moins récemment utilisée au-delà de la limite', () => {
    const c = new Lru<string, number>(2)
    c.set('a', 1)
    c.set('b', 2)
    expect(c.get('a')).toBe(1)
    c.set('c', 3)
    expect([c.size, c.get('a'), c.get('b'), c.get('c')]).toEqual([2, 1, undefined, 3])
  })

  it('ne calcule une valeur absente qu’une fois', () => {
    const c = new Lru<string, number>(1)
    const make = vi.fn(() => 7)
    expect([c.getOrSet('x', make), c.getOrSet('x', make)]).toEqual([7, 7])
    expect(make).toHaveBeenCalledTimes(1)
    c.set('y', 1)
    expect(c.getOrSet('x', make)).toBe(7)
    expect(make).toHaveBeenCalledTimes(2)
  })
})
