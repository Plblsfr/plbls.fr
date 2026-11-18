import { describe, expect, it } from 'vitest'
import { sectionDe } from './battement'
import { hrefs, parseHash } from './route.svelte'

describe('route de la section Stats', () => {
  it('lit #/stats, sa sous-section et son identifiant', () => {
    expect(parseHash('#/stats')).toEqual({ name: 'stats', section: undefined, id: undefined })
    expect(parseHash('#/stats/recaps')).toEqual({ name: 'stats', section: 'recaps', id: undefined })
    expect(parseHash('#/stats/recaps/12')).toEqual({ name: 'stats', section: 'recaps', id: '12' })
  })

  it('écrit les liens de la section', () => {
    expect(hrefs.stats()).toBe('#/stats')
    expect(hrefs.stats('partage')).toBe('#/stats/partage')
    expect(hrefs.stats('recaps', 12)).toBe('#/stats/recaps/12')
  })

  it('le battement range la route dans la section sablier', () => {
    expect(sectionDe(parseHash('#/stats/recaps').name)).toBe('sablier')
  })
})
