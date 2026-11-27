import { hrefs } from '../../lib/route.svelte'
import { shortDuration } from './films'

export type GenreBadge = 'movie' | 'season' | 'series'

export interface FilmBadge {
  kind: GenreBadge
  title_id: string
  season: number | null
  title: string
  season_title: string | null
  year: number | null
  poster: boolean
  season_poster: boolean
  episodes: number | null
  duration: number | null
  obtained_at: string
}

export interface BadgeRef { kind: GenreBadge; title_id: string; season: number | null; obtained_at: string }

export const cleBadge = (b: Pick<BadgeRef, 'kind' | 'title_id' | 'season'>) => (b.kind === 'season' ? `season:${b.title_id}:${b.season}` : `${b.kind}:${b.title_id}`)

export function etiquetteBadge(b: Pick<BadgeRef, 'kind' | 'season'>): string {
  if (b.kind === 'movie') return 'Film terminé'
  if (b.kind === 'season') return `Saison ${b.season} terminée`
  return 'Série terminée'
}

export const iconeBadge = (kind: GenreBadge) => (kind === 'movie' ? 'film' : 'tele')

export function dateBadge(iso: string): string {
  return new Date(iso).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' })
}

export function detailsBadge(b: FilmBadge): string {
  const episodes = b.episodes ? `${b.episodes} épisode${b.episodes > 1 ? 's' : ''}` : ''
  return [b.kind === 'movie' ? b.year : null, episodes, b.duration ? shortDuration(b.duration) : ''].filter(Boolean).join(' · ')
}

export function sousTitreBadge(b: FilmBadge): string {
  if (b.kind !== 'season' || !b.season_title) return ''
  return b.season_title.trim().toLowerCase() === `saison ${b.season}` ? '' : b.season_title
}

export function afficheBadge(b: FilmBadge): { id: string; image: boolean } {
  if (b.kind === 'season' && b.season_poster) return { id: `${b.title_id}~${b.season}`, image: true }
  return { id: b.title_id, image: b.poster }
}

export function couper(texte: string, largeur: number, mesure: (s: string) => number, max = 2): string[] {
  const mots = texte.split(/\s+/).filter(Boolean)
  const lignes: string[] = []
  let courante = ''
  for (const mot of mots) {
    const essai = courante ? `${courante} ${mot}` : mot
    if (!courante || mesure(essai) <= largeur) { courante = essai; continue }
    lignes.push(courante)
    courante = mot
  }
  if (courante) lignes.push(courante)
  if (lignes.length <= max) return lignes
  const gardees = lignes.slice(0, max)
  let derniere = `${gardees[max - 1]} ${lignes.slice(max).join(' ')}`
  while (derniere.length > 1 && mesure(`${derniere}…`) > largeur) derniere = derniere.slice(0, -1).trimEnd()
  gardees[max - 1] = `${derniere}…`
  return gardees
}

export const lienBadge = (b: Pick<BadgeRef, 'kind' | 'title_id' | 'season'>) => `${location.origin}${location.pathname}${hrefs.films('badge', cleBadge(b))}`

export function lireCleBadge(cle: string): Pick<BadgeRef, 'kind' | 'title_id' | 'season'> | null {
  const [genre, id, n, ...reste] = cle.split(':')
  if (reste.length || !id) return null
  if (genre === 'season') return /^[1-9]\d{0,2}$/.test(n ?? '') ? { kind: 'season', title_id: id, season: Number(n) } : null
  if ((genre === 'movie' || genre === 'series') && n === undefined) return { kind: genre, title_id: id, season: null }
  return null
}
