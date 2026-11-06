import { describe, expect, it } from 'vitest'
import { badgeIconProblem, filterBadges, inlineBadges } from './badges'
import type { AdminBadge } from '../../lib/types'

const b = (name: string, sort_order: number, holders: number, created_at: string, description = ''): AdminBadge => ({
  id: name, name, icon: '🏅', color: '#000000', description, icon_version: 0, sort_order, created_at,
  holders: Array.from({ length: holders }, (_, i) => ({ id: `${name}${i}`, pseudo: `${name}${i}` })),
})
const list = [b('Étoile', 2, 1, '2026-01-02'), b('Alpha', 3, 5, '2026-01-01', 'Premier arrivé'), b('Zèbre', 1, 0, '2026-01-03')]

describe('filterBadges', () => {
  it("trie par ordre d'affichage, nom, détenteurs et date de création", () => {
    const names = (s: Parameters<typeof filterBadges>[2]) => filterBadges(list, '', s).map((x) => x.name)
    expect(names('order')).toEqual(['Zèbre', 'Étoile', 'Alpha'])
    expect(names('name')).toEqual(['Alpha', 'Étoile', 'Zèbre'])
    expect(names('holders')).toEqual(['Alpha', 'Étoile', 'Zèbre'])
    expect(names('created')).toEqual(['Zèbre', 'Étoile', 'Alpha'])
  })
  it('cherche dans le nom et la description, sans accent ni casse', () => {
    expect(filterBadges(list, 'etoile', 'name').map((x) => x.name)).toEqual(['Étoile'])
    expect(filterBadges(list, 'ARRIVE', 'name').map((x) => x.name)).toEqual(['Alpha'])
    expect(filterBadges(list, 'zzz', 'name')).toEqual([])
  })
})

describe('badgeIconProblem', () => {
  it('accepte SVG, WebP, PNG et JPEG', () => {
    for (const type of ['image/svg+xml', 'image/webp', 'image/png', 'image/jpeg']) expect(badgeIconProblem({ type, size: 1000 })).toBe('')
  })
  it('refuse les autres formats et les SVG trop lourds', () => {
    expect(badgeIconProblem({ type: 'image/gif', size: 10 })).toMatch(/Format/)
    expect(badgeIconProblem({ type: 'image/svg+xml', size: 70 << 10 })).toMatch(/SVG trop lourd/)
  })
})

describe('inlineBadges', () => {
  it("garde les deux premiers badges et compte les autres", () => {
    expect(inlineBadges(list)).toMatchObject({ extra: 1 })
    expect(inlineBadges(list).shown.map((x) => x.name)).toEqual(['Étoile', 'Alpha'])
    expect(inlineBadges(list.slice(0, 1))).toMatchObject({ extra: 0 })
    expect(inlineBadges([])).toEqual({ shown: [], extra: 0 })
  })
})
