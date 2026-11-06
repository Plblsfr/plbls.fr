import { PROVIDER_LABEL } from '../regarder-ensemble/providers'
import type { VideoList, VideoPost, WatchSession } from '../../lib/types'

/** Sources avec un lecteur intégré : « Regarder ensemble » est possible. Les autres liens restent de simples cartes. */
export const playable = (p: Pick<VideoPost, 'provider'>) => p.provider !== 'simple'

/** Titre affiché d'une carte : jamais le lien brut (titre vide ou égal au lien, cartes anciennes : « Vidéo YouTube »…). */
export const videoTitle = (p: Pick<VideoPost, 'provider' | 'title' | 'url'>) =>
  p.title && p.title !== p.url && !/^https?:\/\//i.test(p.title) ? p.title : p.provider === 'simple' ? 'Lien vidéo' : `Vidéo ${PROVIDER_LABEL[p.provider] ?? 'web'}`

/** Lien à donner à « Regarder ensemble » : le fichier direct annoncé par la page, sinon le lien publié. */
export const playLink = (p: Pick<VideoPost, 'provider' | 'ref' | 'url'>) => (p.provider === 'page' ? p.ref : p.url)

/** Source regroupée pour les filtres : les trois formes de Twitch n'en font qu'une, « page » est un fichier. */
export const sourceOf = (provider: string) => (provider.startsWith('twitch') ? 'twitch' : provider === 'page' ? 'file' : provider)

/** Artiste d'une carte musique : première partie de « Artiste · Album · 3:25 » (une playlist n'en a pas : « 12 titres »). */
export const artistOf = (p: Pick<VideoPost, 'site'>) => (/^\d+ titres?$/.test(p.site) ? '' : p.site.split(' · ')[0])

/** Un morceau YouTube a un identifiant de 11 caractères, une liste ou un album jamais. */
export const isTrackRef = (ref: string) => /^[A-Za-z0-9_-]{11}$/.test(ref)
export const musicLink = (ref: string) => (isTrackRef(ref) ? `https://music.youtube.com/watch?v=${ref}` : `https://music.youtube.com/playlist?list=${ref}`)

/** Mélange de Fisher-Yates, sans modifier la liste d'origine. */
export function shuffled<T>(a: T[], rnd: () => number = Math.random): T[] {
  const out = [...a]
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1))
    ;[out[i], out[j]] = [out[j], out[i]]
  }
  return out
}

export interface VideoFilter { source: string; watched: 'all' | 'seen' | 'unseen'; author: string; list: string; artist: string }
export type VideoSort = 'recent' | 'reactions' | 'title'
export const NO_FILTER: VideoFilter = { source: '', watched: 'all', author: '', list: '', artist: '' }

export function filterPosts(posts: VideoPost[], f: VideoFilter, sort: VideoSort, lists: VideoList[] = []): VideoPost[] {
  const inList = f.list ? new Set(lists.find((l) => l.id === f.list)?.items ?? []) : null
  const out = posts.filter((p) =>
    (!f.source || sourceOf(p.provider) === f.source) &&
    (f.watched === 'all' || (f.watched === 'seen') === !!p.watched_at) &&
    (!f.author || p.author.id === f.author) &&
    (!f.artist || artistOf(p) === f.artist) &&
    (!inList || inList.has(p.message_id)))
  // ids de messages : plus grand = plus récent (comparaison numérique, les ids dépassent parfois 2^31 mais restent sûrs en nombre)
  const byRecent = (a: VideoPost, b: VideoPost) => Number(b.message_id) - Number(a.message_id)
  const byTitle = (a: VideoPost, b: VideoPost) => videoTitle(a).localeCompare(videoTitle(b), 'fr') || byRecent(a, b)
  return out.sort(sort === 'reactions' ? (a, b) => b.reactions - a.reactions || byRecent(a, b) : sort === 'title' ? byTitle : byRecent)
}

/** Vidéos d'une liste dans l'ordre de la liste, sans celles supprimées du salon. */
export function listPosts(l: VideoList, posts: VideoPost[]): VideoPost[] {
  const by = new Map(posts.map((p) => [p.message_id, p]))
  return l.items.flatMap((id) => by.get(id) ?? [])
}

/** Déplace l'élément `i` de `d` rang (-1 : vers le haut) ; la liste est renvoyée telle quelle si le déplacement sort des bornes. */
export function moved<T>(a: T[], i: number, d: number): T[] {
  const j = i + d
  if (j < 0 || j >= a.length) return a
  const out = [...a]
  ;[out[i], out[j]] = [out[j], out[i]]
  return out
}

export const countRsvp = (s: Pick<WatchSession, 'rsvps'>) => {
  const c = { yes: 0, maybe: 0, no: 0 }
  for (const a of Object.values(s.rsvps)) c[a]++
  return c
}

/** Une séance est « en cours » entre son heure et 3 h plus tard (au-delà, le serveur ne la liste plus). */
export const sessionPhase = (s: Pick<WatchSession, 'starts_at' | 'started_at'>, now = Date.now()): 'upcoming' | 'live' =>
  s.started_at || Date.parse(s.starts_at) <= now ? 'live' : 'upcoming'

/** « dans 12 min », « dans 3 h », « demain 20:30 »… pour l'en-tête et la barre latérale. */
export function when(iso: string, now = Date.now()): string {
  const t = Date.parse(iso)
  const min = Math.round((t - now) / 60000)
  if (min <= 0) return 'maintenant'
  if (min < 60) return `dans ${min} min`
  const d = new Date(t)
  const hm = d.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' })
  const days = Math.round((new Date(d).setHours(0, 0, 0, 0) - new Date(now).setHours(0, 0, 0, 0)) / 86400000)
  if (days === 0) return `aujourd’hui ${hm}`
  if (days === 1) return `demain ${hm}`
  return `${d.toLocaleDateString('fr-FR', { weekday: 'short', day: 'numeric', month: 'short' })} ${hm}`
}

/** Valeur d'un `<input type="datetime-local">` (heure locale) pour une date. */
export const localInput = (d: Date) => new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16)
