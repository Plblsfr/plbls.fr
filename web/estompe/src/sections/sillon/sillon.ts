import { ApiError, imageAt, requestAt } from '../../lib/api'
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
  imageAjout: (id: string, size: Taille) => imageAt(SILLON_URL, `/additions/images/${enc(id)}?size=${size}`),
  lecture: (id: string, b: DemandeLecture) => send<Lecture>('POST', `/tracks/${enc(id)}/playback`, b),
  preferences: () => get<Preferences>('/preferences'),
  qualite: (quality: Qualite) => send<Preferences>('PUT', '/preferences', { quality }),
  decouverteActive: (discovery: boolean) => send<Preferences>('PUT', '/preferences', { discovery }),

  playlists: () => get<{ items: Playlist[] }>('/playlists'),
  playlist: (id: string) => get<FichePlaylist>(`/playlists/${enc(id)}`),
  creerPlaylist: (name: string) => send<FichePlaylist>('POST', '/playlists', { name }),
  renommerPlaylist: (id: string, name: string) => send<FichePlaylist>('PATCH', `/playlists/${enc(id)}`, { name }),
  supprimerPlaylist: (id: string) => send<void>('DELETE', `/playlists/${enc(id)}`),
  ajouterAPlaylist: (id: string, contenu: Contenu, allow_duplicates = false) => send<FichePlaylist & { added: number; duplicates: string[] }>('POST', `/playlists/${enc(id)}/tracks`, { ...contenu, allow_duplicates }),
  retirerDePlaylist: (id: string, item: string) => send<FichePlaylist>('DELETE', `/playlists/${enc(id)}/tracks/${enc(item)}`),
  deplacerDansPlaylist: (id: string, from: number, to: number) => send<FichePlaylist>('POST', `/playlists/${enc(id)}/move`, { from, to }),

  likes: () => get<Likes>('/likes'),
  titresLikes: () => get<TitresLikes>('/likes/tracks'),
  liker: (kind: keyof Likes, id: string, oui: boolean) => send<void>(oui ? 'PUT' : 'DELETE', `/likes/${kind}/${enc(id)}`),

  file: () => get<FileServeur>('/queue'),
  remplacerFile: (b: Record<string, unknown>) => send<FileServeur>('PUT', '/queue', b),
  modifierFile: (b: Record<string, unknown>) => send<FileServeur>('PATCH', '/queue', b),
  ajouterAFile: (b: Record<string, unknown>) => send<FileServeur>('POST', '/queue/tracks', b),
  retirerDeFile: (b: Record<string, unknown>) => send<FileServeur>('POST', '/queue/remove', b),
  deplacerDansFile: (b: Record<string, unknown>) => send<FileServeur>('POST', '/queue/move', b),

  ecoutes: (cursor?: string | null) => get<Page<Ecoute>>(`/history?${page(cursor)}`),
  enregistrerEcoute: (b: EcouteEnvoyee) => send<{ id: string }>('POST', '/history', b),
  topPistes: (period: Periode) => get<{ items: { track: Piste; plays: number }[] }>(`/history/top/tracks?period=${period}&limit=20`),
  topArtistes: (period: Periode) => get<{ items: { artist: Artiste; plays: number }[] }>(`/history/top/artists?period=${period}&limit=20`),

  nePlusProposer: (track: string) => send<void>('POST', '/recommendations/dismiss', { track }),
  similaires: (id: string, limit = 30) => get<{ items: Piste[] }>(`/tracks/${enc(id)}/similar?limit=${limit}`),

  chercherAjout: (q: string) => get<ResultatsAjout>(`/additions/search?q=${enc(q)}`),
  ajouter: (kind: 'album' | 'artist', mbid: string) => send<Ajout & { existing?: boolean }>('POST', '/additions', { kind, mbid }),
  mesAjouts: () => get<{ items: Ajout[] }>('/additions/mine'),

  disque: () => get<Disque>('/admin/disk'),
  ajoutsAdmin: () => get<{ items: Ajout[] }>('/admin/additions'),
  relancerAjout: (id: string) => send<Ajout>('POST', `/admin/additions/${enc(id)}/retry`),
  albumsAdmin: (neverPlayed: boolean) => get<{ items: (Album & { size: number; added_by: string | null })[] }>(`/admin/albums?limit=100${neverPlayed ? '&never_played=true' : ''}`),
  supprimerAlbum: (id: string) => send<void>('DELETE', `/admin/albums/${enc(id)}`),
  supprimerArtiste: (id: string) => send<void>('DELETE', `/admin/artists/${enc(id)}`),
  statut: () => get<Statut>('/admin/status'),
  rejetes: () => get<{ items: { name: string; reason: string; seen_at: string }[] }>('/admin/rejected'),
  synchroniser: () => send<{ started: boolean }>('POST', '/admin/sync'),
  analyse: () => get<Analyse>('/admin/analysis'),
  relancerAnalyse: () => send<{ requeued: number }>('POST', '/admin/analysis/retry'),
  decouverte: () => get<Decouverte>('/admin/discovery'),
}

export interface Preferences { quality: Qualite; qualities: Qualite[]; discovery: boolean }

export interface Playlist { id: string; name: string; description: string | null; track_count: number; duration: number | null; cover_albums: string[]; created_at: string; updated_at: string }
export interface ElementPlaylist { id: string; position: number; added_at: string; track: Piste }
export type FichePlaylist = Playlist & { items: ElementPlaylist[] }
export type Contenu = { tracks: string[] } | { album: string } | { artist: string } | { playlist: string }

export interface Likes { tracks: string[]; albums: string[]; artists: string[] }
export interface TitresLikes { name: string; track_count: number; duration: number | null; cover_albums: string[]; items: { liked_at: string; track: Piste }[] }

export type Suggestions = 'off' | 'light' | 'frequent'
export const SUGGESTIONS: { id: Suggestions; label: string }[] = [
  { id: 'off', label: 'Désactivées' },
  { id: 'light', label: 'Légères' },
  { id: 'frequent', label: 'Fréquentes' },
]
export type TypeSource = 'album' | 'playlist' | 'artist' | 'likes' | 'search'
export interface ElementFile { id: string; suggested: boolean; discovery: boolean; track: Piste }
export const marqueDe = (e: Pick<ElementFile, 'suggested' | 'discovery'> | null | undefined) => (e?.discovery ? 'Découverte' : e?.suggested ? 'Suggestion' : null)
export type Repetition = 'off' | 'all' | 'one'
export const REPETITIONS: { id: Repetition; label: string }[] = [
  { id: 'off', label: 'Sans répétition' },
  { id: 'all', label: 'Répéter la file' },
  { id: 'one', label: 'Répéter le titre' },
]
export interface FileServeur {
  revision: number
  source: { type: TypeSource; id: string | null; name: string } | null
  index: number
  position: number
  shuffle: boolean
  repeat: Repetition
  suggestions: Suggestions
  seed: number | null
  updated_at: string | null
  items: ElementFile[]
}

export type Periode = 'week' | 'month' | 'year' | 'all'
export interface Ecoute { id: string; track: Piste; started_at: string; played: number; ended: 'completed' | 'skipped' | 'interrupted'; counted: boolean; source: { type: TypeSource; id: string | null } | null; suggested: boolean }
export interface EcouteEnvoyee { id: string; track: string; played: number; ended_by: 'end' | 'skip' | 'stop'; source?: { type: TypeSource; id?: string }; item?: string }

export interface ImageAbsent { mbid: string; image: string | null; library: { id: string; image: boolean } | null }
export interface ArtisteAbsent extends ImageAbsent { name: string | null; disambiguation: string | null; type: string | null; overview: string | null; in_library: boolean; in_progress: boolean; excluded: boolean }
export interface AlbumAbsent extends ImageAbsent { title: string | null; type: string | null; secondary_types: string[]; year: number | null; duration: number | null; artist: { mbid: string; name: string | null }; in_library: boolean; in_progress: boolean; excluded: boolean; estimated_size: number | null }
export interface ResultatsAjout { artists: ArtisteAbsent[]; albums: AlbumAbsent[] }
export type EtatAjout = 'sent' | 'downloading' | 'partial' | 'available' | 'failed' | 'deleted'
export interface Ajout { id: string; kind: 'album' | 'artist'; mbid: string; title: string; artist: { mbid: string; name: string }; origin: 'manual' | 'discovery'; state: EtatAjout; reason: string | null; estimated_size: number; albums: string[]; image: string | null; created_at: string; updated_at: string; author?: { sub: string; name: string } }
export interface Occupation { used: number; pending: number; limit: number | null; free: number | null }
export type Disque = Occupation & { daily_limit: number; configured: boolean }
export interface Statut { sync: { source: string; succeeded_at: string | null; attempted_at: string | null; error: string | null }[]; running: boolean; configured: { lidarr: boolean; webhook: boolean }; library: { artists: number; albums: number; tracks: number; absent_tracks: number; files: number; rejected: number; size: number } }
export interface Analyse { version: number; parallel: number; paused: boolean; tasks: { pending: number; running: number; done: number; failed: number }; tracks: number; analysed: number; loaded: number; errors: { track: string; title: string; error: string | null; attempts: number; at: string }[] }
export interface Decouverte {
  enabled: boolean
  budget: { used: number; limit: number; daily_limit: number; trial_days: number }
  items: (Ajout & { author: { sub: string; name: string }; similar_to: string | null; track_title: string | null; size: number })[]
  removal_candidates: { addition: string; album: Album; size: number; author: { sub: string; name: string }; similar_to: string | null; track_title: string | null; skips: number; available_since: string | null }[]
}

const nombre = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : null)

export function octets(n: number | null | undefined) {
  if (n == null) return '—'
  const unites = ['o', 'Ko', 'Mo', 'Go', 'To']
  let v = n
  let i = 0
  while (v >= 1000 && i < unites.length - 1) { v /= 1000; i++ }
  return `${i ? v.toFixed(v < 10 ? 1 : 0).replace('.', ',') : v} ${unites[i]}`
}

export function occupation(o: Occupation) {
  return o.limit ? `${octets(o.used + o.pending)} sur ${octets(o.limit)}` : `${octets(o.used + o.pending)}, sans plafond`
}

export const vignetteAbsent = (o: ImageAbsent) =>
  o.library ? { id: o.library.id, image: o.library.image, ajout: null } : { id: o.mbid, image: false, ajout: o.image }

export function refusAjout(e: unknown): string {
  if (!(e instanceof ApiError)) return 'L’ajout n’a pas pu être envoyé.'
  if (e.code === 'discovery_budget') return 'Budget de la découverte atteint.'
  if (e.status === 507) {
    const d = e.data
    const o: Occupation = { used: nombre(d.used) ?? 0, pending: nombre(d.pending) ?? 0, limit: nombre(d.limit), free: nombre(d.free) }
    const besoin = nombre(d.estimated)
    return `Disque plein : ${occupation(o)} occupés${o.free != null ? `, ${octets(o.free)} libres` : ''}${besoin ? ` ; cet ajout demande environ ${octets(besoin)}` : ''}.`
  }
  if (e.status === 429) return `Limite d’ajouts atteinte : ${e.message || 'réessaie demain'}.`
  if (e.code === 'already_available') return 'Déjà dans la bibliothèque.'
  if (e.code === 'excluded') return 'Retiré par un administrateur : il ne peut pas être ajouté à nouveau.'
  if (e.code === 'invalid_state') return 'Cet ajout est déjà en cours.'
  if (e.status === 502) return 'Lidarr ne répond pas, réessaie plus tard.'
  if (e.status === 503) return 'L’ajout de musique n’est pas configuré.'
  return e.message || 'L’ajout n’a pas pu être envoyé.'
}

export const ETATS_AJOUT: Record<EtatAjout, string> = {
  sent: 'Envoyé',
  downloading: 'Téléchargement',
  partial: 'Partiel',
  available: 'Disponible',
  failed: 'Échec',
  deleted: 'Supprimé',
}

export function deplacerListe<T>(liste: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || from >= liste.length || to < 0 || to >= liste.length) return liste
  const r = [...liste]
  const [x] = r.splice(from, 1)
  r.splice(to, 0, x)
  return r
}

export const dureeTotale = (secondes: number | null | undefined) => {
  const m = Math.round((secondes ?? 0) / 60)
  return m >= 60 ? `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}` : `${m} min`
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

export const totalAlbum = (pistes: Piste[]) => dureeTotale(pistes.reduce((n, p) => n + (p.available ? p.duration ?? 0 : 0), 0))

export const titresDe = (n: number) => `${n} titre${n > 1 ? 's' : ''}`
