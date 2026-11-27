import { fichierAt, requestAt } from '../../lib/api'
import type { GenreBadge } from '../entracte/badges'
import { config } from '../../lib/config'

export const SABLIER_URL = config.sablierUrl
export const sablierOn = () => !!SABLIER_URL

export const PLATEFORMES = ['web', 'pwa', 'windows', 'android'] as const
export type Plateforme = (typeof PLATEFORMES)[number]

export const SECTIONS = ['messagerie', 'entracte', 'sillon', 'regie', 'sablier', 'admin', 'autre'] as const
export type Section = (typeof SECTIONS)[number]

export interface SablierMe {
  sub: string
  name: string
  email: string
  permissions: { access: boolean; admin: boolean; entracte?: boolean; sillon?: boolean }
  sections: string[]
  heartbeat_seconds?: number
}

export type Periode = 'week' | 'month' | 'year' | 'all'
export type Domaine = 'usage' | 'entracte' | 'sillon' | 'messaging'
export type Unite = 'count' | 'seconds' | 'days' | 'date'

export interface JourDuree { day: string; seconds: number }
export interface Plage { period: string; from: string; to: string }

export interface Usage extends Plage {
  seconds: number
  by_day: JourDuree[]
  by_section: { section: string; seconds: number }[]
  by_platform: { platform: string; seconds: number }[]
  longest_day: JourDuree | null
  active_days: number
  first_day: string | null
}

export interface SansAcces { access: false }

export interface Entracte extends Plage {
  access: true
  seconds: number
  movies_watched: number
  episodes_watched: number
  series_completed: number
  longest_session: number
  biggest_day: JourDuree | null
  most_episodes_in_a_row: number
  abandoned: number
  intros_skipped: number
  cinema_seconds: number
  cinema_sessions: number
}

export interface Sillon extends Plage {
  access: true
  seconds: number
  tracks_played: number
  distinct_tracks: number
}

export interface Salon { server: string; channel: string }

export interface Messagerie extends Plage {
  total: number
  private: number
  by_channel: (Salon & { messages: number })[]
  top_channel: (Salon & { messages: number }) | null
  busiest_day: { day: string; messages: number } | null
  reactions_given: number
  reactions_received: number
  top_emoji: { emoji: string; count: number } | null
  files: number
  voice_seconds: number
  private_voice_seconds: number
  longest_call: number
  top_voice_channel: (Salon & { seconds: number }) | null
  screen_share_seconds: number
}

export interface Stats {
  usage: Usage
  entracte: Entracte | SansAcces
  sillon: Sillon | SansAcces
  messaging: Messagerie
}

export interface Progres { value: number; next_threshold: number; unit: Unite }

export interface TropheeDebloque {
  id: number
  key: string
  name: string
  description: string
  icon: string
  hidden: boolean
  tier: number
  tiers: number
  unlocked_at: string
  seen: boolean
  shared: boolean
  progress?: Progres
}

export interface TropheeEnCours extends Progres {
  id: number
  key: string
  name: string
  description: string
  icon: string
  tier: number
  tiers: number
}

export interface Trophees { unlocked: TropheeDebloque[]; in_progress: TropheeEnCours[]; hidden_remaining: number }

export interface Serie { current: number; best: number }
export interface Series { global: Serie; entracte: Serie | null; sillon: Serie | null; messaging: Serie }

export interface NonVus { trophies: number; recaps: number }
export interface Vus { trophies?: number[] | 'all'; recaps?: number[] | 'all' }

export interface RecapResume { id: number; kind: 'month' | 'year'; period: string; created_at: string; seen: boolean }

export interface Fait { kind: string; value: number; day?: string; server?: string; channel?: string; emoji?: string }
export interface TropheePublic { name: string; description: string; icon: string; tier: number; unlocked_at: string }

export interface Recap {
  id: number
  kind: 'month' | 'year'
  period: string
  from: string
  to: string
  usage: Usage
  entracte?: Omit<Entracte, 'access'>
  sillon?: Omit<Sillon, 'access'>
  messaging: Messagerie
  highlights: Fait[]
  trophies: TropheePublic[]
  streak: { global?: number; entracte?: number; sillon?: number; messaging?: number }
}

export interface StatPartagee { key: string; label: string; domain: Domaine; shared: boolean }
export interface TropheePartage { id: number; name: string; icon: string; shared: boolean }
export interface BadgePublic { key: string; kind: GenreBadge; title_id: string; season: number | null; obtained_at: string }
export interface BadgePartage extends BadgePublic { shared: boolean }
export interface Partage { stats: StatPartagee[]; trophies: TropheePartage[]; badges?: BadgePartage[] }
export interface ChoixPartage { stats?: Record<string, boolean>; trophies?: Record<string, boolean>; badges?: Record<string, boolean> }

export interface ValeurPublique { key: string; label: string; domain: Domaine; unit: Unite; value: number }
export interface Profil { month: ValeurPublique[]; total: ValeurPublique[]; first_day?: string; trophies: TropheePublic[]; badges?: BadgePublic[] }

export interface Vue { date: string; admin_name: string; action: 'consultation' | 'effacement' }

export type Annuaire = 'aegis' | 'local'
export interface Personne { sub: string; name: string | null; known: boolean; last_seen: string | null; first_day: string | null; last_day: string | null; active_seconds_total: number }
export interface Personnes { directory: Annuaire; people: Personne[] }

export interface TotauxJour { day: string; active_people: number; usage_seconds: number; watched_seconds: number; listened_seconds: number; messages: number }
export interface Totaux extends Plage { active_people: number; usage_seconds: number; watched_seconds: number; listened_seconds: number; messages: number; by_day: TotauxJour[] }

export interface EntreeJournal { id: number; date: string; action: 'consultation' | 'effacement'; admin_sub: string; admin_name: string; target_sub: string; target_name: string | null }
export interface Journal { directory: Annuaire; entries: EntreeJournal[]; next_before: number | null }

export type GenreTrophee = 'tiers' | 'exploit'
export type GrainTrophee = 'day' | 'week' | 'month' | 'all'
export interface TropheeAdmin {
  id: number
  key: string
  name: string
  description: string
  icon: string
  hidden: boolean
  active: boolean
  kind: GenreTrophee
  stat: string | null
  period: GrainTrophee | null
  thresholds: number[]
  exploit: string | null
  parameter: number | null
  order: number
  unlocked_by: number
}
export interface SaisieTrophee {
  key?: string
  name?: string
  description?: string
  icon?: string
  hidden?: boolean
  active?: boolean
  kind?: GenreTrophee
  stat?: string
  period?: GrainTrophee
  thresholds?: number[]
  exploit?: string
  parameter?: number
}
export interface StatCatalogue { key: string; label: string; domain: Domaine; unit: Unite; shareable: boolean; tiers: boolean }
export interface ExploitCatalogue { key: string; label: string; condition: string; domain: Domaine; unit: Unite; default_parameter: number | null }
export interface Catalogue { stats: StatCatalogue[]; exploits: ExploitCatalogue[]; periods: GrainTrophee[] }

export const MOT_EFFACER = 'EFFACER'

const options = { discret: true }
const appel = <T>(methode: string, chemin: string, corps?: unknown) => requestAt<T>(SABLIER_URL, methode, chemin, corps, options)
const enc = encodeURIComponent
const lire = <T>(chemin: string, periode: Periode, date: string) => appel<T>('GET', `${chemin}?period=${periode}&date=${enc(date)}`)

const lectures = (prefixe: string) => ({
  usage: (periode: Periode, date: string) => lire<Usage>(`${prefixe}/usage`, periode, date),
  entracte: (periode: Periode, date: string) => lire<Entracte | SansAcces>(`${prefixe}/entracte`, periode, date),
  sillon: (periode: Periode, date: string) => lire<Sillon | SansAcces>(`${prefixe}/sillon`, periode, date),
  messagerie: (periode: Periode, date: string) => lire<Messagerie>(`${prefixe}/messaging`, periode, date),
  trophees: () => appel<Trophees>('GET', `${prefixe}/trophies`),
  series: () => appel<Series>('GET', `${prefixe}/streaks`),
  recaps: () => appel<RecapResume[]>('GET', `${prefixe}/recaps`),
  recap: (id: number) => appel<Recap>('GET', `${prefixe}/recaps/${id}`),
})

export type LecturesSablier = ReturnType<typeof lectures>
export const lecturesDe = (sub: string): LecturesSablier => lectures(`/admin/people/${enc(sub)}`)

export const sablierApi = {
  me: () => appel<SablierMe>('GET', '/me'),
  battre: (platform: Plateforme, section: Section) => appel<void>('POST', '/heartbeat', { platform, section }),
  ...lectures(''),
  nonVus: () => appel<NonVus>('GET', '/unseen'),
  marquerVus: (vus: Vus) => appel<void>('POST', '/seen', vus),
  partage: () => appel<Partage>('GET', '/sharing'),
  partager: (choix: ChoixPartage) => appel<Partage>('PUT', '/sharing', choix),
  profil: (id: string) => appel<Profil>('GET', `/profiles/${enc(id)}`),
  vues: () => appel<Vue[]>('GET', '/views'),
  exporter: () => fichierAt(SABLIER_URL, '/export', 'sablier-export.json', options),
  effacer: () => appel<void>('DELETE', '/data', { confirm: MOT_EFFACER }),
}

export type ClientSablier = typeof sablierApi

export const adminApi = {
  personnes: () => appel<Personnes>('GET', '/admin/people'),
  effacer: (sub: string) => appel<void>('DELETE', `/admin/people/${enc(sub)}/data`, { confirm: MOT_EFFACER }),
  totaux: (periode: Periode, date: string) => lire<Totaux>('/admin/totals', periode, date),
  journal: (avant?: number | null, limite = 50) => appel<Journal>('GET', `/admin/audit?limit=${limite}${avant ? `&before=${avant}` : ''}`),
  trophees: () => appel<TropheeAdmin[]>('GET', '/admin/trophies'),
  creer: (t: SaisieTrophee) => appel<TropheeAdmin>('POST', '/admin/trophies', t),
  modifier: (id: number, t: SaisieTrophee) => appel<TropheeAdmin>('PATCH', `/admin/trophies/${id}`, t),
  catalogue: () => appel<Catalogue>('GET', '/admin/catalogue'),
}

export function enregistrer(blob: Blob, nom: string) {
  const lien = document.createElement('a')
  lien.href = URL.createObjectURL(blob)
  lien.download = nom
  lien.click()
  URL.revokeObjectURL(lien.href)
}
