import { describe, expect, it } from 'vitest'
import { DUREES, STYLES, dessinerBadge, detailsAnim, paletteDe, styleDe } from './badges-anim'
import type { FilmBadge } from './badges'

const badge = (b: Partial<FilmBadge> = {}): FilmBadge => ({
  kind: 'season', title_id: 'daredevil', season: 1, title: 'Marvel’s Daredevil', season_title: 'Saison 1', year: 2015, poster: true, season_poster: false,
  episodes: 13, duration: 13 * 3240, obtained_at: '2026-11-12T22:40:00.000Z', ...b,
})

function contexte() {
  const textes: string[] = []
  const images: unknown[] = []
  const ctx = new Proxy({} as Record<string, unknown>, {
    get: (cible, nom) => {
      if (nom === 'fillText') return (t: string) => textes.push(t)
      if (nom === 'drawImage') return (i: unknown) => images.push(i)
      if (nom === 'measureText') return (t: string) => ({ width: t.length * 20 })
      if (nom === 'createLinearGradient' || nom === 'createRadialGradient') return () => ({ addColorStop() {} })
      if (nom in cible) return cible[nom as string]
      return () => {}
    },
    set: (cible, nom, v) => { cible[nom as string] = v; return true },
  }) as unknown as CanvasRenderingContext2D
  return { ctx, textes, images }
}

const palette = { bg: '#140405', fg: '#fbeaea', ac: '#e3262f', ac2: '#ff8a65' }
const polices = { titre: 'sans-serif', texte: 'sans-serif', serif: 'serif', mono: 'monospace' }

describe('animations des badges', () => {
  it('donne toujours le même style à un même titre, et tous les styles servent', () => {
    expect(styleDe('daredevil')).toBe(styleDe('daredevil'))
    const vus = new Set(Array.from({ length: 200 }, (_, i) => styleDe(`titre-${i}`)))
    expect([...vus].sort()).toEqual([...STYLES].sort())
  })

  it('dessine chaque style à chaque instant sans erreur, et finit sur le titre et l’étiquette', () => {
    const affiche = { width: 600, height: 900 } as unknown as CanvasImageSource & { width: number; height: number }
    for (const style of STYLES) {
      for (let t = 0; t <= DUREES[style] + 1; t += 0.25) dessinerBadge(contexte().ctx, style, badge(), affiche, palette, polices, t)
      const fin = contexte()
      dessinerBadge(fin.ctx, style, badge(), affiche, palette, polices, DUREES[style])
      const tout = fin.textes.join(' | ')
      expect(tout, style).toContain('Daredevil')
      expect(tout.toUpperCase(), style).toMatch(/SAISON 1|FIN/)
      expect(fin.images.length, style).toBeGreaterThan(0)
      const sans = contexte()
      dessinerBadge(sans.ctx, style, badge({ poster: false }), null, palette, polices, DUREES[style])
      expect(sans.images, style).toEqual([])
    }
  })

  it('détaille un film et une série', () => {
    expect(detailsAnim(badge())).toBe('13 épisodes · 11 h 42')
    expect(detailsAnim(badge({ kind: 'movie', season: null, episodes: null, year: 2001, duration: 7320 }))).toBe('2001 · 2 h 02')
  })

  it('tire la palette de l’affiche, ou de la teinte du titre sans affiche', () => {
    const rouge = new Uint8ClampedArray(Array.from({ length: 24 * 36 }, () => [200, 20, 30, 255]).flat())
    const p = paletteDe(rouge, 'x')
    expect(p.bg).toMatch(/^#[0-9a-f]{6}$/)
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(p.ac.slice(i, i + 2), 16))
    expect(r).toBeGreaterThan(g + 80)
    expect(r).toBeGreaterThan(b + 80)
    expect(paletteDe(null, 'daredevil')).toEqual(paletteDe(null, 'daredevil'))
    expect(paletteDe(new Uint8ClampedArray(0), 'x').bg).toMatch(/^#/)
  })
})
