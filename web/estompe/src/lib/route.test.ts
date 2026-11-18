import { describe, expect, it } from 'vitest'
import { sectionDe } from './battement'
import { hrefs, parseHash } from './route.svelte'

describe('route de la section Stats', () => {
  it('lit #/stats, sa sous-section et son identifiant', () => {
    expect(parseHash('#/stats')).toEqual({ name: 'stats', section: undefined, id: undefined, detail: undefined })
    expect(parseHash('#/stats/recaps')).toEqual({ name: 'stats', section: 'recaps', id: undefined, detail: undefined })
    expect(parseHash('#/stats/recaps/12')).toEqual({ name: 'stats', section: 'recaps', id: '12', detail: undefined })
    expect(parseHash('#/stats/admin/personnes/kc%2Fa')).toEqual({ name: 'stats', section: 'admin', id: 'personnes', detail: 'kc/a' })
  })

  it('écrit les liens de la section', () => {
    expect(hrefs.stats()).toBe('#/stats')
    expect(hrefs.stats('partage')).toBe('#/stats/partage')
    expect(hrefs.stats('recaps', 12)).toBe('#/stats/recaps/12')
    expect(hrefs.stats('admin', 'personnes', 'kc/a')).toBe('#/stats/admin/personnes/kc%2Fa')
  })

  it('le battement range la route dans la section sablier', () => {
    expect(sectionDe(parseHash('#/stats/recaps').name)).toBe('sablier')
  })
})
