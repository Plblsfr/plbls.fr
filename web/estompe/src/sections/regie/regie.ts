import { requestAt } from '../../lib/api'
import { config } from '../../lib/config'

export const REGIE_URL = config.regieUrl
export const regieOn = () => !!REGIE_URL

export interface RegieMe {
  sub: string
  name: string
  email: string
  permissions: { admin: boolean }
}

export type Level = 'notice' | 'warning' | 'error'
export type ServiceKey = 'radarr' | 'sonarr' | 'lidarr' | 'prowlarr' | 'qbittorrent'

export interface Alert { level: Level; source: string; message: string }
export interface Transfer { download_speed: number; upload_speed: number; connection: string }
export interface Service {
  key: ServiceKey
  name: string
  configured: boolean
  reachable: boolean
  version: string | null
  error: string | null
  alerts: Alert[]
  transfer?: Transfer
}
export interface Indexer {
  id: number
  name: string
  enabled: boolean
  protocol: string | null
  privacy: string | null
  failing: boolean
  disabled_until: string | null
  last_failure: string | null
}
export interface Disk { service: ServiceKey; path: string; free: number | null; accessible: boolean }
export interface Blocked { service: ServiceKey; id: number; title: string; level: 'warning' | 'error'; state: string | null; messages: string[] }
export interface RegieStatus { services: Service[]; indexers: Indexer[]; disks: Disk[]; queue: Blocked[] }
export interface IndexerTest { ok: boolean; message: string | null }

export type Tone = 'ok' | 'att' | 'dgr' | ''

const SERVICE_NAMES: Record<ServiceKey, string> = { radarr: 'Radarr', sonarr: 'Sonarr', lidarr: 'Lidarr', prowlarr: 'Prowlarr', qbittorrent: 'qBittorrent' }
export const serviceName = (k: ServiceKey) => SERVICE_NAMES[k] ?? k

/** État d'un service en une étiquette : non configuré, injoignable, puis la pire de ses alertes. */
export function serviceState(s: Service): { label: string; tone: Tone } {
  if (!s.configured) return { label: 'Non configuré', tone: '' }
  if (!s.reachable) return { label: 'Injoignable', tone: 'dgr' }
  if (s.alerts.some((a) => a.level === 'error')) return { label: 'En erreur', tone: 'dgr' }
  if (s.error || s.alerts.some((a) => a.level === 'warning')) return { label: 'Avertissement', tone: 'att' }
  return { label: 'OK', tone: 'ok' }
}

export function indexerState(i: Indexer): { label: string; tone: Tone } {
  if (!i.enabled) return { label: 'Désactivé', tone: '' }
  if (i.failing) return { label: 'En panne', tone: 'dgr' }
  return { label: 'OK', tone: 'ok' }
}

/** En panne d'abord, puis actifs, puis désactivés ; par nom dans chaque groupe. */
export function sortIndexers(items: Indexer[]): Indexer[] {
  const rang = (i: Indexer) => (!i.enabled ? 2 : i.failing ? 0 : 1)
  return [...items].sort((a, b) => rang(a) - rang(b) || a.name.localeCompare(b.name, 'fr'))
}

const UNITES = ['o', 'Ko', 'Mo', 'Go', 'To']
export function bytes(octets: number | null): string {
  if (octets === null || !Number.isFinite(octets) || octets < 0) return ''
  let v = octets
  let u = 0
  while (v >= 1024 && u < UNITES.length - 1) { v /= 1024; u++ }
  return `${v.toLocaleString('fr-FR', { maximumFractionDigits: v >= 100 || u === 0 ? 0 : 1 })} ${UNITES[u]}`
}
export const speed = (octets: number) => `${bytes(octets) || '0 o'}/s`

/** Ce qui demande une action : services en défaut, indexeurs actifs en panne, dossiers inaccessibles, téléchargements bloqués. */
export function problems(s: RegieStatus): number {
  return s.services.filter((x) => x.configured && serviceState(x).tone !== 'ok').length
    + s.indexers.filter((i) => i.enabled && i.failing).length
    + s.disks.filter((d) => !d.accessible).length
    + s.queue.length
}

const get = <T>(p: string) => requestAt<T>(REGIE_URL, 'GET', p)

export const regieApi = {
  me: () => get<RegieMe>('/me'),
  status: () => get<RegieStatus>('/admin/status'),
  testIndexer: (id: number) => requestAt<IndexerTest>(REGIE_URL, 'POST', `/admin/indexers/${id}/test`),
  removeFromQueue: (service: ServiceKey, id: number, retry: boolean) => requestAt<void>(REGIE_URL, 'DELETE', `/admin/queue/${service}/${id}${retry ? '?retry=true' : ''}`),
}
