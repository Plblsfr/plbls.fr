import { describe, expect, it } from 'vitest'
import { adjacent, moveServer, orderServers } from './serverorder'

const s = (...ids: string[]) => ids.map((id) => ({ id }))

describe('ordre des serveurs', () => {
  it('suit l’ordre choisi et garde les nouveaux serveurs à la fin', () => {
    expect(orderServers(s('a', 'b', 'c', 'd'), ['c', 'a', 'zz']).map((x) => x.id)).toEqual(['c', 'a', 'b', 'd'])
    expect(orderServers(s('a', 'b'), []).map((x) => x.id)).toEqual(['a', 'b'])
  })
  it('déplace un serveur à la place d’un autre', () => {
    expect(moveServer(['a', 'b', 'c'], 'a', 'c')).toEqual(['b', 'c', 'a'])
    expect(moveServer(['a', 'b', 'c'], 'c', 'a')).toEqual(['c', 'a', 'b'])
    expect(moveServer(['a', 'b'], 'a', 'x')).toEqual(['a', 'b'])
  })
  it('passe au voisin en bouclant', () => {
    expect(adjacent(['h', 'a', 'b'], 'b', 1)).toBe('h')
    expect(adjacent(['h', 'a', 'b'], 'h', -1)).toBe('b')
    expect(adjacent(['h', 'a'], 'zz', 1)).toBe('h')
    expect(adjacent([], 'a', 1)).toBeUndefined()
  })
})
