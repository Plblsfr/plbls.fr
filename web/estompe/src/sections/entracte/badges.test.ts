import { describe, expect, it } from 'vitest'
import { afficheBadge, cleBadge, couper, dessinerCarte, detailsBadge, etiquetteBadge, nomFichierBadge, sousTitreBadge, texteBadge, type FilmBadge } from './badges'

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
    expect(texteBadge(badge())).toBe('Film terminé : Le Grand Film (2021), le 2 mars 2026.')
    expect(nomFichierBadge(badge({ title: 'Amélie & Cie !' }))).toBe('badge-amelie-cie.png')
    expect(nomFichierBadge(saison)).toBe('badge-la-serie-saison-2.png')
    expect(nomFichierBadge(badge({ kind: 'series', title: '???' }))).toBe('badge-titre-serie.png')
  })

  it('coupe un titre en lignes et abrège ce qui dépasse', () => {
    const mesure = (s: string) => s.length
    expect(couper('Un titre court', 20, mesure)).toEqual(['Un titre court'])
    expect(couper('Un titre bien plus long que la ligne', 15, mesure)).toEqual(['Un titre bien', 'plus long que…'])
    expect(couper('Anticonstitutionnellement', 10, mesure)).toEqual(['Anticonstitutionnellement'])
    expect(couper('', 10, mesure)).toEqual([])
  })

  it('dessine la carte : étiquette, titre, détails, date et signature', () => {
    const textes: string[] = []
    const images: unknown[] = []
    const ctx = new Proxy({} as Record<string, unknown>, {
      get: (cible, nom) => {
        if (nom === 'fillText') return (t: string) => textes.push(t)
        if (nom === 'drawImage') return (i: unknown) => images.push(i)
        if (nom === 'measureText') return (t: string) => ({ width: t.length * 30 })
        if (nom === 'createRadialGradient') return () => ({ addColorStop() {} })
        if (nom in cible) return cible[nom as string]
        return () => {}
      },
      set: (cible, nom, v) => { cible[nom as string] = v; return true },
    }) as unknown as CanvasRenderingContext2D
    const affiche = {} as CanvasImageSource
    dessinerCarte(ctx, badge(), affiche, 'serif')
    expect(images).toEqual([affiche])
    expect(textes).toEqual(['FILM TERMINÉ', 'Le Grand Film', '2021 · 2 h 13', 'Regardé jusqu’au bout · 2 mars 2026', 'Entracte · plbls.fr'])
    textes.length = 0
    dessinerCarte(ctx, badge({ poster: false }), null, 'serif')
    expect(textes.slice(0, 1)).toEqual(['Le Grand Film'])
  })
})
