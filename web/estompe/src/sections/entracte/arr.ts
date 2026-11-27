import type { EntracteRequest } from './entracte'

export type ArrService = 'radarr' | 'sonarr'
export interface ArrError { service: ArrService; message: string }
export interface ArrEpisodeRef { id: number | null; season: number | null; number: number | null; title: string | null }
export interface ArrSeason { number: number; monitored: boolean; episodes: number; episode_files: number; size: number }

export interface ArrTitle {
  service: ArrService
  id: number
  tmdb_id: number | null
  title: string
  year: number | null
  monitored: boolean
  profile: number | null
  folder: string | null
  status: string | null
  size: number
  has_file?: boolean
  available?: boolean
  file?: { quality: string | null; size: number | null } | null
  episodes?: number
  episode_files?: number
  monitor_new?: boolean
  seasons?: ArrSeason[]
}

export interface ArrEpisode { id: number; season: number; number: number; title: string | null; monitored: boolean; has_file: boolean; air_date: string | null; quality: string | null; size: number | null }
export interface ArrSheet extends ArrTitle { profiles: { id: number; name: string | null }[]; episode_list?: ArrEpisode[] }

export interface ArrRelease {
  guid: string
  indexer_id: number
  title: string
  indexer: string | null
  protocol: string | null
  quality: string | null
  languages: string[]
  formats: string[]
  score: number
  size: number
  seeders: number | null
  leechers: number | null
  age: number
  approved: boolean
  rejections: string[]
  full_season: boolean
}

export type QueueState = 'downloading' | 'queued' | 'paused' | 'importing' | 'blocked'
interface ArrOwner { service: ArrService; title_id: number | null; tmdb_id: number | null; title: string | null; year: number | null }
export interface ArrQueueItem extends ArrOwner {
  id: number
  episode: ArrEpisodeRef | null
  release: string
  quality: string | null
  indexer: string | null
  client: string | null
  protocol: string | null
  size: number
  left: number
  progress: number
  state: QueueState
  messages: string[]
  eta: string | null
  importable: boolean
}
export interface ArrMissing extends ArrOwner { profile?: number | null; last_search: string | null; episode?: ArrEpisodeRef; air_date?: string | null }
export type ArrEvent = 'grabbed' | 'imported' | 'failed' | 'deleted' | 'renamed' | 'ignored' | 'other'
export interface ArrHistory extends ArrOwner { event: ArrEvent; episode: ArrEpisodeRef | null; release: string | null; quality: string | null; date: string | null; indexer: string | null; reason: string | null }
export interface ArrBlocked extends ArrOwner { id: number; release: string | null; quality: string | null; date: string | null; indexer: string | null; reason: string | null }
export interface ArrImportFile { name: string; size: number; quality: string | null; languages: string[]; title: string | null; episodes: ArrEpisodeRef[]; rejections: string[] }

export type ReleaseTarget =
  | { service: 'radarr'; title: number; label: string }
  | { service: 'sonarr'; title: number; season: number; label: string }
  | { service: 'sonarr'; episode: number; label: string }

export const serviceName = (s: ArrService) => (s === 'radarr' ? 'Radarr' : 'Sonarr')
export const serviceOf = (kind: 'movie' | 'series'): ArrService => (kind === 'movie' ? 'radarr' : 'sonarr')

export function releaseQuery(t: ReleaseTarget): string {
  if ('episode' in t) return `episode=${t.episode}`
  return 'season' in t ? `title=${t.title}&season=${t.season}` : `title=${t.title}`
}

export function bytes(octets: number | null | undefined): string {
  if (!octets || octets <= 0) return ''
  const unites = ['o', 'Ko', 'Mo', 'Go', 'To']
  let v = octets
  let i = 0
  while (v >= 1024 && i < unites.length - 1) { v /= 1024; i++ }
  return `${v.toLocaleString('fr-FR', { maximumFractionDigits: v >= 100 || i < 3 ? 0 : 1 })} ${unites[i]}`
}

const deux = (n: number) => String(n).padStart(2, '0')
export function episodeCode(e: Pick<ArrEpisodeRef, 'season' | 'number'> | null | undefined): string {
  if (!e || e.season === null || e.number === null) return ''
  return `S${deux(e.season)}E${deux(e.number)}`
}

export function titleLabel(o: Pick<ArrOwner, 'title' | 'year'> & { episode?: ArrEpisodeRef | null }): string {
  const base = `${o.title ?? 'Titre inconnu'}${o.year ? ` (${o.year})` : ''}`
  const code = episodeCode(o.episode)
  return code ? `${base} · ${code}` : base
}

const PALIERS: [number, Intl.RelativeTimeFormatUnit][] = [[60, 'second'], [60, 'minute'], [24, 'hour'], [30, 'day'], [12, 'month'], [Infinity, 'year']]
export function since(date: string | null | undefined, now = Date.now()): string {
  if (!date) return 'jamais'
  let ecart = (new Date(date).getTime() - now) / 1000
  if (Number.isNaN(ecart)) return 'jamais'
  const format = new Intl.RelativeTimeFormat('fr-FR', { numeric: 'auto' })
  for (const [taille, unite] of PALIERS) {
    if (Math.abs(ecart) < taille) return format.format(Math.round(ecart), unite)
    ecart /= taille
  }
  return 'jamais'
}

export function age(jours: number): string {
  if (jours <= 0) return 'aujourd’hui'
  return jours === 1 ? '1 jour' : `${jours.toLocaleString('fr-FR')} jours`
}

const QUEUE: Record<QueueState, [string, string]> = {
  downloading: ['Téléchargement', 'info'],
  queued: ['En attente', 'att'],
  paused: ['En pause', 'att'],
  importing: ['Import en cours', 'ok'],
  blocked: ['Bloqué', 'dgr'],
}
export const queueLabel = (s: QueueState) => QUEUE[s]?.[0] ?? s
export const queueTone = (s: QueueState) => QUEUE[s]?.[1] ?? ''

const EVENTS: Record<ArrEvent, [string, string]> = {
  grabbed: ['Récupéré', 'info'],
  imported: ['Importé', 'ok'],
  failed: ['Échec', 'dgr'],
  deleted: ['Fichier supprimé', ''],
  renamed: ['Renommé', ''],
  ignored: ['Ignoré', 'att'],
  other: ['Autre', ''],
}
export const eventLabel = (e: ArrEvent) => EVENTS[e]?.[0] ?? e
export const eventTone = (e: ArrEvent) => EVENTS[e]?.[1] ?? ''

export function titleState(t: ArrTitle, queue: ArrQueueItem[] = []): { label: string; tone: string } {
  if (queue.some((q) => q.service === t.service && q.title_id === t.id)) return { label: 'En cours', tone: 'info' }
  if (t.service === 'radarr') {
    if (t.has_file) return { label: 'Présent', tone: 'ok' }
    return t.monitored ? { label: 'Manquant', tone: 'att' } : { label: 'Non surveillé', tone: '' }
  }
  const manque = (t.episodes ?? 0) - (t.episode_files ?? 0)
  if (manque > 0) return { label: `${manque} manquant${manque > 1 ? 's' : ''}`, tone: 'att' }
  return t.monitored ? { label: 'Complète', tone: 'ok' } : { label: 'Non surveillée', tone: '' }
}

export const isMissing = (t: ArrTitle) => (t.service === 'radarr' ? !t.has_file && t.monitored : (t.episodes ?? 0) > (t.episode_files ?? 0))

export type Tracking =
  | { kind: 'none' }
  | { kind: 'downloading'; progress: number; note: string }
  | { kind: 'blocked'; note: string }
  | { kind: 'missing'; note: string }

/** Où en est une demande envoyée : ce que la file de Radarr/Sonarr télécharge pour elle, ou rien du tout. */
export function trackRequest(r: Pick<EntracteRequest, 'kind' | 'tmdb_id' | 'seasons' | 'state' | 'sent'>, titles: ArrTitle[], queue: ArrQueueItem[]): Tracking {
  if (r.state !== 'acceptee' || !r.sent) return { kind: 'none' }
  const service = serviceOf(r.kind)
  const miens = queue.filter((q) => q.service === service && q.tmdb_id === r.tmdb_id && (!r.seasons?.length || q.episode?.season == null || r.seasons.includes(q.episode.season)))
  const bloque = miens.find((q) => q.state === 'blocked')
  if (bloque) return { kind: 'blocked', note: `${episodeCode(bloque.episode) ? `${episodeCode(bloque.episode)} : ` : ''}${bloque.messages[0] ?? 'téléchargement bloqué'}` }
  if (miens.length) {
    const progress = Math.round(miens.reduce((n, q) => n + q.progress, 0) / miens.length)
    const premier = miens[0]
    const note = miens.length === 1 ? [premier.quality, bytes(premier.size), queueLabel(premier.state).toLowerCase()].filter(Boolean).join(' · ') : `${miens.length} téléchargements dans la file`
    return { kind: 'downloading', progress, note }
  }
  const titre = titles.find((t) => t.service === service && t.tmdb_id === r.tmdb_id)
  if (!titre) return { kind: 'none' }
  return isMissing(titre) ? { kind: 'missing', note: 'Aucun téléchargement en cours.' } : { kind: 'none' }
}

export const titleOf = (r: Pick<EntracteRequest, 'kind' | 'tmdb_id'>, titles: ArrTitle[]) => titles.find((t) => t.service === serviceOf(r.kind) && t.tmdb_id === r.tmdb_id) ?? null

export interface MissingGroup { key: string; title_id: number; title: string; season: number; episodes: ArrMissing[] }
export function splitMissing(items: ArrMissing[]): { movies: ArrMissing[]; groups: MissingGroup[] } {
  const groups: MissingGroup[] = []
  for (const m of items) {
    if (m.service !== 'sonarr' || m.title_id === null || m.episode?.season == null) continue
    const key = `${m.title_id}:${m.episode.season}`
    let g = groups.find((x) => x.key === key)
    if (!g) { g = { key, title_id: m.title_id, title: titleLabel({ title: m.title, year: m.year }), season: m.episode.season, episodes: [] }; groups.push(g) }
    g.episodes.push(m)
  }
  for (const g of groups) g.episodes.sort((a, b) => (a.episode?.number ?? 0) - (b.episode?.number ?? 0))
  return { movies: items.filter((m) => m.service === 'radarr'), groups }
}

export type ReleaseSort = 'score' | 'seeders' | 'size' | 'age'
export interface ReleaseFilter { approved: '' | 'yes' | 'no'; resolution: string; language: string; sort: ReleaseSort }
export const resolutionOf = (r: Pick<ArrRelease, 'quality' | 'title'>) => /(2160|1080|720|480)p/i.exec(`${r.quality ?? ''} ${r.title}`)?.[1] ?? ''
export const releaseLanguages = (items: ArrRelease[]) => [...new Set(items.flatMap((r) => r.languages))].sort((a, b) => a.localeCompare(b, 'fr'))

export function filterReleases(items: ArrRelease[], f: ReleaseFilter): ArrRelease[] {
  const gardees = items.filter((r) => (f.approved === '' || r.approved === (f.approved === 'yes')) && (!f.resolution || resolutionOf(r) === f.resolution) && (!f.language || r.languages.includes(f.language)))
  if (f.sort === 'score') return gardees
  const cle = (r: ArrRelease) => (f.sort === 'seeders' ? -(r.seeders ?? -1) : f.sort === 'size' ? -r.size : r.age)
  return gardees.toSorted((a, b) => cle(a) - cle(b))
}

export type CatalogFilter = { service: '' | ArrService; state: '' | 'missing' | 'unmonitored'; text: string }
export function filterTitles(items: ArrTitle[], f: CatalogFilter): ArrTitle[] {
  const texte = f.text.trim().toLocaleLowerCase('fr')
  return items
    .filter((t) => (!f.service || t.service === f.service) && (f.state === '' || (f.state === 'missing' ? isMissing(t) : !t.monitored)) && (!texte || t.title.toLocaleLowerCase('fr').includes(texte)))
    .sort((a, b) => a.title.localeCompare(b.title, 'fr'))
}
