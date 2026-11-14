import type { FilmItem } from './films'

export interface EntracteMe {
  sub: string
  name: string
  email: string
  permissions: { access: boolean; request: boolean; admin: boolean }
  push: { web: boolean; fcm: boolean }
}

export type RequestState = 'en_attente' | 'acceptee' | 'refusee' | 'disponible'
export type RequestKind = 'movie' | 'series'

export interface EntracteRequest {
  id: string
  kind: RequestKind
  tmdb_id: number
  seasons: number[] | null
  title: string
  year: number | null
  poster_url: string | null
  state: RequestState
  reason: string | null
  created_at: string
  updated_at: string
  title_id?: string | null
  joined_existing?: boolean
  sent?: boolean
  send_error?: string | null
  requesters?: { name: string; joined_at: string }[]
}

/** Réglages d'envoi proposés par Radarr ou Sonarr, et ceux qu'ils utilisent déjà le plus (`defaults`). */
export interface SendOptions {
  folders: { path: string; free: number | null }[]
  profiles: { id: number; name: string }[]
  tags: { id: number; label: string }[]
  defaults: { folder: string | null; profile: number | null; tags: number[]; availability: string | null }
}
export interface SendChoice { folder: string; profile: number; tags: number[] }

/** Choix présenté à l'ouverture : les réglages habituels de Radarr/Sonarr, à défaut les premiers proposés ; rien s'il manque un dossier ou un profil. */
export function initialChoice(o: SendOptions): SendChoice | null {
  const folder = o.folders.find((f) => f.path === o.defaults.folder) ?? o.folders[0]
  const profile = o.profiles.find((p) => p.id === o.defaults.profile) ?? o.profiles[0]
  if (!folder || !profile) return null
  return { folder: folder.path, profile: profile.id, tags: o.defaults.tags.filter((t) => o.tags.some((x) => x.id === t)) }
}

export function freeSpace(octets: number | null): string {
  if (octets === null || octets < 0) return ''
  const go = octets / 1024 ** 3
  const texte = go >= 1000 ? `${(go / 1024).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} To` : `${Math.round(go).toLocaleString('fr-FR')} Go`
  return `${texte} libres`
}

export interface SearchResult extends Partial<Omit<FilmItem, 'id' | 'kind' | 'title' | 'year'>> {
  id: string | null
  kind: 'movie' | 'series' | 'episode'
  title: string
  year: number | null
  present: boolean
  tmdb_id: number
  poster_url?: string | null
  requestable: boolean
  request?: EntracteRequest | null
}

export interface RequestableSeason {
  number: number
  title: string
  episode_count: number
  available_count: number
  complete: boolean
  request_state: RequestState | null
}

export interface RequestableSeries {
  tmdb_id: number
  title: string
  year: number | null
  poster_url: string | null
  seasons: RequestableSeason[]
}

export interface SyncStatus { source: string; succeeded_at: string | null; attempted_at: string | null; error: string | null }
export interface EntracteStatus {
  sync: SyncStatus[]
  transcoding: { requested: string; active: string; message: string | null; running: number }
  configured: { radarr: boolean; sonarr: boolean; tmdb: boolean; push: boolean }
}

export interface EntracteNotification { id: number; kind: string; request_id: string | null; title: string; body: string; created_at: string }

export const TMDB_ATTRIBUTION = 'Métadonnées et affiches fournies par TMDB. Entracte utilise l’API de TMDB sans être approuvé ni certifié par TMDB.'

const STATE_LABEL: Record<RequestState, string> = {
  en_attente: 'En attente',
  acceptee: 'Acceptée',
  refusee: 'Refusée',
  disponible: 'Disponible',
}
export const stateLabel = (s: RequestState) => STATE_LABEL[s] ?? s
export const stateTone = (s: RequestState) => (s === 'disponible' ? 'ok' : s === 'refusee' ? 'dgr' : s === 'acceptee' ? 'info' : '')

export function seasonsLabel(r: Pick<EntracteRequest, 'kind' | 'seasons'>): string {
  if (r.kind === 'movie') return 'Film'
  if (!r.seasons?.length) return 'Série entière'
  return r.seasons.length === 1 ? `Saison ${r.seasons[0]}` : `Saisons ${r.seasons.join(', ')}`
}

/** Présents d'abord (dans l'ordre de la bibliothèque), puis absents, que l'on peut demander. */
export function splitSearch(items: SearchResult[]): { present: SearchResult[]; absent: SearchResult[] } {
  return { present: items.filter((i) => i.present), absent: items.filter((i) => !i.present) }
}

/** Saisons qu'on peut encore demander : ni complètes, ni déjà demandées. */
export function requestableSeasons(s: RequestableSeries): RequestableSeason[] {
  return s.seasons.filter((x) => !x.complete && !x.request_state)
}

/** Corps de la demande : série entière si toutes les saisons demandables sont cochées et qu'aucune n'est déjà là. */
export function requestBody(kind: RequestKind, tmdb_id: number, chosen: number[], series?: RequestableSeries) {
  if (kind === 'movie') return { kind, tmdb_id }
  const all = series ? requestableSeasons(series).map((x) => x.number) : []
  const whole = !!series && series.seasons.every((x) => !x.complete && !x.request_state && x.available_count === 0) && all.length > 0 && all.every((n) => chosen.includes(n))
  return whole ? { kind, tmdb_id } : { kind, tmdb_id, seasons: [...new Set(chosen)].sort((a, b) => a - b) }
}

/** Notifications à afficher depuis le dernier relevé (app Windows, sans push). */
export function freshNotifications(items: EntracteNotification[], lastSeen: number): EntracteNotification[] {
  return items.filter((n) => n.id > lastSeen).sort((a, b) => a.id - b.id)
}

/** Sections de l'app selon les permissions : la messagerie d'Estompe et Entracte, chacune seulement si accordée. */
export function sections(estompe: boolean, entracte: EntracteMe | null) {
  return { messagerie: estompe, entracte: !!entracte?.permissions.access }
}
