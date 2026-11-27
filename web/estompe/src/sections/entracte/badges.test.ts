import { describe, expect, it } from 'vitest'
import { afficheBadge, cleBadge, couper, detailsBadge, etiquetteBadge, lienBadge, lireCleBadge, sousTitreBadge, type FilmBadge } from './badges'

const badge = (b: Partial<FilmBadge> = {}): FilmBadge => ({
  kind: 'movie', title_id: 'f1', season: null, title: 'Le Grand Film', season_title: null, year: 2021, poster: true, season_poster: false,
  episodes: null, duration: 7980, obtained_at: '2026-03-02T20:00:00.000Z', ...b,
})

describe('badges', () => {
  it('nomme, détaille et identifie chaque genre de badge', () => {
    const saison = badge({ kind: 'season', title_id: 's1', season: 2, title: 'La Série', year: 2019, season_title: 'Saison 2', season_poster: true, episodes: 8, duration: 8 * 2700 })
    const serie = badge({ kind: 'series', title_id: 's1', title: 'La Série', episodes: 1, duration: 1800 })
    expect([badge(), saison, serie].map(etiquetteBadge)).toEqual(['Film terminé', 'Saison 2 terminée', 'Série terminée'])
    expect([badge(), saison, serie].map(cleBadge)).toEqual(['movie:f1', 'season:s1:2', 'series:s1'])
    expect(detailsBadge(badge())).toBe('2021 · 2 h 13')
    expect(detailsBadge(saison)).toBe('8 épisodes · 6 h 00')
    expect(detailsBadge(serie)).toBe('1 épisode · 30 min')
    expect(sousTitreBadge(saison)).toBe('')
    expect(sousTitreBadge({ ...saison, season_title: 'Le retour' })).toBe('Le retour')
    expect(afficheBadge(saison)).toEqual({ id: 's1~2', image: true })
    expect(afficheBadge({ ...saison, season_poster: false, poster: false })).toEqual({ id: 's1', image: false })
  })

  it('fabrique le lien d’un badge et relit sa clé', () => {
    expect(lienBadge({ kind: 'season', title_id: 's1', season: 2 })).toBe(`${location.origin}${location.pathname}#/films/badge/season%3As1%3A2`)
    expect(lireCleBadge('season:s1:2')).toEqual({ kind: 'season', title_id: 's1', season: 2 })
    expect(lireCleBadge('movie:f1')).toEqual({ kind: 'movie', title_id: 'f1', season: null })
    for (const faux of ['season:s1', 'season:s1:0', 'movie:f1:1', 'episode:e1', 'movie', '']) expect(lireCleBadge(faux), faux).toBeNull()
  })

  it('coupe un titre en lignes et abrège ce qui dépasse', () => {
    const mesure = (s: string) => s.length
    expect(couper('Un titre court', 20, mesure)).toEqual(['Un titre court'])
    expect(couper('Un titre bien plus long que la ligne', 15, mesure)).toEqual(['Un titre bien', 'plus long que…'])
    expect(couper('Anticonstitutionnellement', 10, mesure)).toEqual(['Anticonstitutionnellement'])
    expect(couper('', 10, mesure)).toEqual([])
  })
})
