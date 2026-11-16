import { imageAt, requestAt } from '../../lib/api'
import { config } from '../../lib/config'

export const SILLON_URL = config.sillonUrl
export const sillonOn = () => !!SILLON_URL

export interface SillonMe {
  sub: string
  name: string
  email: string
  permissions: { access: boolean; add: boolean; admin: boolean }
  sections: string[]
}

export interface Artiste {
  id: string
  name: string
  overview: string | null
  genres: string[]
  image: boolean
  fanart: boolean
  available: boolean
  album_count: number
}

export interface Album {
  id: string
  title: string
  type: string | null
  secondary_types: string[]
  release_date: string | null
  year: number | null
  genres: string[]
  cover: boolean
  available: boolean
  added_at: string | null
  track_count: number
  duration: number | null
  artist: { id: string; name: string }
}

export interface Piste {
  id: string
  title: string
  disc: number
  number: number | null
  duration: number | null
  explicit: boolean
  available: boolean
  album: { id: string; title: string; year: number | null; cover: boolean }
  artist: { id: string; name: string }
  format: { codec: string; container: string | null; bitrate: number | null; sample_rate: number | null; channels: number | null; bits: number | null } | null
}

export type FicheArtiste = Artiste & { albums: Album[] }
export type FicheAlbum = Album & { tracks: Piste[] }
export interface Page<T> { items: T[]; next_cursor: string | null }
export interface Resultats { artists: Artiste[]; albums: Album[]; tracks: Piste[] }

export type Qualite = 'originale' | 'haute' | 'normale' | 'basse'
export const QUALITES: { id: Qualite; label: string }[] = [
  { id: 'originale', label: 'Originale' },
  { id: 'haute', label: 'Haute (256 kb/s)' },
  { id: 'normale', label: 'Normale (160 kb/s)' },
  { id: 'basse', label: 'Basse (96 kb/s)' },
]

export interface Support { codecs: string[]; containers: string[] }

export interface Lecture {
  track: string
  url: string
  expires_at: string
  mode: 'direct' | 'transcode'
  quality: Qualite
  format: { codec: string; container: string; mime: string; bitrate: number | null }
  duration: number | null
  samples: number | null
  sample_rate: number | null
  output_sample_rate: number | null
  encoder_delay: number
  gain: { reference: number; loudness: number | null; track: number | null; track_peak: number | null; album: number | null; album_peak: number | null }
}

export type DemandeLecture = Support & { quality?: Qualite }

const enc = encodeURIComponent
const get = <T>(p: string) => requestAt<T>(SILLON_URL, 'GET', p)
const send = <T>(m: string, p: string, b?: unknown) => requestAt<T>(SILLON_URL, m, p, b)
const page = (cursor?: string | null, limit = 50) => new URLSearchParams({ limit: String(limit), ...(cursor ? { cursor } : {}) })

export const sillonApi = {
  me: () => get<SillonMe>('/me'),
  artistes: (cursor?: string | null) => get<Page<Artiste>>(`/artists?${page(cursor)}`),
  artiste: (id: string) => get<FicheArtiste>(`/artists/${enc(id)}`),
  recents: (cursor?: string | null, limit = 50) => get<Page<Album>>(`/albums/recent?${page(cursor, limit)}`),
  album: (id: string) => get<FicheAlbum>(`/albums/${enc(id)}`),
  piste: (id: string) => get<Piste>(`/tracks/${enc(id)}`),
  recherche: (q: string) => get<Resultats>(`/search?q=${enc(q)}&limit=20`),
  pochette: (id: string, size: Taille) => imageAt(SILLON_URL, `/albums/${enc(id)}/cover?size=${size}`),
  portrait: (id: string, size: Taille, type: 'poster' | 'fanart' = 'poster') => imageAt(SILLON_URL, `/artists/${enc(id)}/image?type=${type}&size=${size}`),
  lecture: (id: string, b: DemandeLecture) => send<Lecture>('POST', `/tracks/${enc(id)}/playback`, b),
  preferences: () => get<{ quality: Qualite; qualities: Qualite[] }>('/preferences'),
  qualite: (quality: Qualite) => send<{ quality: Qualite; qualities: Qualite[] }>('PUT', '/preferences', { quality }),
}

export type Taille = 'small' | 'medium' | 'large'

const FORMATS: [string, string, string][] = [
  ['audio/flac', 'flac', 'flac'],
  ['audio/mpeg', 'mp3', 'mp3'],
  ['audio/ogg; codecs="opus"', 'opus', 'ogg'],
  ['audio/ogg; codecs="vorbis"', 'vorbis', 'ogg'],
  ['audio/ogg; codecs="flac"', 'flac', 'ogg'],
  ['audio/webm; codecs="opus"', 'opus', 'webm'],
  ['audio/mp4; codecs="mp4a.40.2"', 'aac', 'mp4'],
  ['audio/mp4; codecs="alac"', 'alac', 'mp4'],
  ['audio/mp4; codecs="flac"', 'flac', 'mp4'],
  ['audio/aac', 'aac', 'aac'],
  ['audio/wav', 'pcm', 'wav'],
  ['audio/aiff', 'pcm', 'aiff'],
]

export function support(peutLire: (type: string) => string): Support {
  const codecs = new Set<string>()
  const containers = new Set<string>()
  for (const [type, codec, conteneur] of FORMATS) {
    if (!peutLire(type)) continue
    codecs.add(codec)
    containers.add(conteneur)
  }
  return { codecs: [...codecs], containers: [...containers] }
}

export function gainLineaire(l: Lecture, album: boolean): number {
  const g = l.gain
  const db = (album ? g.album ?? g.track : g.track) ?? 0
  const pic = album && g.album != null ? g.album_peak : g.track_peak
  return 10 ** (Math.min(db, pic == null ? db : -1 - pic) / 20)
}

export const debut = (l: Lecture) => (l.encoder_delay && l.output_sample_rate ? l.encoder_delay / l.output_sample_rate : 0)

export const dureeExacte = (l: Lecture | null, p: Piste | null) => (l?.samples && l.sample_rate ? l.samples / l.sample_rate : l?.duration ?? p?.duration ?? 0)

export function sections(estompe: boolean, me: SillonMe | null) {
  return { messagerie: estompe, sillon: !!me?.permissions.access }
}

export const disque = (pistes: Piste[]) => new Set(pistes.map((p) => p.disc)).size > 1

export function anneeEtType(a: Pick<Album, 'year' | 'type' | 'secondary_types'>) {
  const type = a.secondary_types.length ? a.secondary_types.join(', ') : a.type === 'EP' ? 'EP' : a.type === 'Single' ? 'Single' : a.type === 'Album' ? 'Album' : a.type ?? ''
  return [a.year, type].filter(Boolean).join(' · ')
}

export function totalAlbum(pistes: Piste[]) {
  const s = pistes.reduce((n, p) => n + (p.available ? p.duration ?? 0 : 0), 0)
  const m = Math.round(s / 60)
  return m >= 60 ? `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}` : `${m} min`
}

export const titresDe = (n: number) => `${n} titre${n > 1 ? 's' : ''}`
