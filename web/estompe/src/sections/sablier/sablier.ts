import { requestAt } from '../../lib/api'
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
export interface Partage { stats: StatPartagee[]; trophies: TropheePartage[] }
export interface ChoixPartage { stats?: Record<string, boolean>; trophies?: Record<string, boolean> }

export interface ValeurPublique { key: string; label: string; domain: Domaine; unit: Unite; value: number }
export interface Profil { month: ValeurPublique[]; total: ValeurPublique[]; first_day?: string; trophies: TropheePublic[] }

const appel = <T>(methode: string, chemin: string, corps?: unknown) => requestAt<T>(SABLIER_URL, methode, chemin, corps, { discret: true })
const lire = <T>(chemin: string, periode: Periode, date: string) => appel<T>('GET', `${chemin}?period=${periode}&date=${encodeURIComponent(date)}`)

export const sablierApi = {
  me: () => appel<SablierMe>('GET', '/me'),
  battre: (platform: Plateforme, section: Section) => appel<void>('POST', '/heartbeat', { platform, section }),
  usage: (periode: Periode, date: string) => lire<Usage>('/usage', periode, date),
  entracte: (periode: Periode, date: string) => lire<Entracte | SansAcces>('/entracte', periode, date),
  sillon: (periode: Periode, date: string) => lire<Sillon | SansAcces>('/sillon', periode, date),
  messagerie: (periode: Periode, date: string) => lire<Messagerie>('/messaging', periode, date),
  trophees: () => appel<Trophees>('GET', '/trophies'),
  series: () => appel<Series>('GET', '/streaks'),
  nonVus: () => appel<NonVus>('GET', '/unseen'),
  marquerVus: (vus: Vus) => appel<void>('POST', '/seen', vus),
  recaps: () => appel<RecapResume[]>('GET', '/recaps'),
  recap: (id: number) => appel<Recap>('GET', `/recaps/${id}`),
  partage: () => appel<Partage>('GET', '/sharing'),
  partager: (choix: ChoixPartage) => appel<Partage>('PUT', '/sharing', choix),
  profil: (sub: string) => appel<Profil>('GET', `/profiles/${encodeURIComponent(sub)}`),
}

export type ClientSablier = typeof sablierApi
