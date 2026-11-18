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

export interface TorrentTitle { service: ServiceKey; title: string; detail: string | null }
export interface Torrent {
  hash: string
  name: string
  state: string
  progress: number
  size: number
  download_speed: number
  upload_speed: number
  eta: number
  ratio: number
  category: string
  tags: string[]
  added_at: number
  seeds: number
  peers: number
  download_limit: number
  upload_limit: number
  seeding_time?: number
  uploaded?: number
  title: TorrentTitle | null
}
export interface GlobalTransfer { download_speed: number; upload_speed: number; download_limit: number; upload_limit: number; alternative: boolean; free_space: number | null; connection: string }
export interface Downloads { version: number; torrents?: Torrent[]; categories?: string[]; tags?: string[]; transfer?: GlobalTransfer; error: string | null }
export interface TorrentFile { index: number; name: string; size: number; progress: number; priority: number }
export interface TorrentDetail { hash: string; files: TorrentFile[]; title: TorrentTitle | null }

export type TorrentGroup = 'downloading' | 'seeding' | 'stopped' | 'checking' | 'error'
export const GROUPS: { id: TorrentGroup; label: string }[] = [
  { id: 'downloading', label: 'En cours' },
  { id: 'seeding', label: 'En partage' },
  { id: 'stopped', label: 'Suspendus' },
  { id: 'checking', label: 'Vérification' },
  { id: 'error', label: 'En erreur' },
]

/** État de qBittorrent (4 et 5 : `paused*` ou `stopped*`) ramené à un groupe, un libellé et un ton. */
export function torrentState(state: string): { group: TorrentGroup; label: string; tone: Tone } {
  switch (state) {
    case 'error': return { group: 'error', label: 'En erreur', tone: 'dgr' }
    case 'missingFiles': return { group: 'error', label: 'Fichiers manquants', tone: 'dgr' }
    case 'pausedDL': case 'stoppedDL': return { group: 'stopped', label: 'Suspendu', tone: '' }
    case 'pausedUP': case 'stoppedUP': return { group: 'stopped', label: 'Terminé, suspendu', tone: '' }
    case 'checkingDL': case 'checkingUP': case 'checkingResumeData': return { group: 'checking', label: 'Vérification', tone: 'att' }
    case 'moving': return { group: 'checking', label: 'Déplacement', tone: 'att' }
    case 'allocating': return { group: 'checking', label: 'Allocation', tone: 'att' }
    case 'uploading': case 'forcedUP': return { group: 'seeding', label: 'Envoi', tone: 'ok' }
    case 'stalledUP': case 'queuedUP': return { group: 'seeding', label: 'En partage', tone: 'ok' }
    case 'metaDL': case 'forcedMetaDL': return { group: 'downloading', label: 'Métadonnées', tone: 'att' }
    case 'stalledDL': return { group: 'downloading', label: 'En attente de sources', tone: 'att' }
    case 'queuedDL': return { group: 'downloading', label: 'En file', tone: '' }
    case 'downloading': case 'forcedDL': return { group: 'downloading', label: 'Réception', tone: 'ok' }
    default: return { group: 'downloading', label: state || 'Inconnu', tone: '' }
  }
}
export const isStopped = (t: Torrent) => torrentState(t.state).group === 'stopped'

export function torrentLabel(t: Torrent): { title: string; sub: string } {
  if (!t.title) return { title: t.name, sub: '' }
  return { title: [t.title.title, t.title.detail].filter(Boolean).join(' · '), sub: `${serviceName(t.title.service)} · ${t.name}` }
}

/** Filtre par groupe (vide = tous) et par texte, sur le nom du torrent, le titre rattaché, la catégorie et les tags. */
export function filterTorrents(items: Torrent[], group: TorrentGroup | '', query: string): Torrent[] {
  const mots = query.toLocaleLowerCase('fr').split(/\s+/).filter(Boolean)
  return items.filter((t) => {
    if (group && torrentState(t.state).group !== group) return false
    if (!mots.length) return true
    const texte = [t.name, t.title?.title, t.title?.detail, t.category, ...t.tags].filter(Boolean).join(' ').toLocaleLowerCase('fr')
    return mots.every((m) => texte.includes(m))
  })
}

export function groupCounts(items: Torrent[]): Record<TorrentGroup, number> {
  const n: Record<TorrentGroup, number> = { downloading: 0, seeding: 0, stopped: 0, checking: 0, error: 0 }
  for (const t of items) n[torrentState(t.state).group]++
  return n
}

const ETA_INFINI = 8_640_000
export function eta(secondes: number): string {
  if (!Number.isFinite(secondes) || secondes <= 0 || secondes >= ETA_INFINI) return ''
  if (secondes < 60) return '< 1 min'
  const m = Math.round(secondes / 60)
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h} h ${String(m % 60).padStart(2, '0')}`
  return `${Math.floor(h / 24)} j ${h % 24} h`
}

/** Durée de partage actif : « 45 min », « 3 h 05 », « 2 j 4 h » ; vide si nulle ou inconnue. */
export function seedingTime(secondes: number | undefined): string {
  if (!secondes || !Number.isFinite(secondes) || secondes < 60) return ''
  const m = Math.floor(secondes / 60)
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h} h ${String(m % 60).padStart(2, '0')}`
  return `${Math.floor(h / 24)} j ${h % 24} h`
}

export const percent = (p: number) => `${(Math.floor(Math.min(Math.max(p, 0), 1) * 1000) / 10).toLocaleString('fr-FR')} %`
export const PRIORITIES: { value: number; label: string }[] = [{ value: 0, label: 'Ignoré' }, { value: 1, label: 'Normale' }, { value: 6, label: 'Haute' }, { value: 7, label: 'Maximale' }]
/** Limite saisie en Ko/s (vide ou 0 = sans limite) rendue en octets par seconde ; `null` si la saisie n'est pas un nombre positif. */
export function limitBytes(saisie: string): number | null {
  const t = saisie.trim().replace(',', '.')
  if (!t) return 0
  const n = Number(t)
  return Number.isFinite(n) && n >= 0 && n <= 9_000_000 ? Math.round(n * 1024) : null
}
export const limitInput = (octets: number) => (octets > 0 ? String(Math.round(octets / 1024)) : '')

export type DepositState = 'upload' | 'checking' | 'seeding' | 'mismatch'
export interface DepositFile { index: number; path: string; size: number; received: number; valid: number | null }
export interface Deposit {
  id: string
  hash?: string
  name: string
  state: DepositState
  created_at: number
  tracker: string | null
  private: boolean
  size: number
  received: number
  files: DepositFile[]
  imported: ServiceKey[]
  error: string | null
}
export interface Deposits { items: Deposit[]; used: number; limit: number }
export interface ImportItem { path: string; size: number | null; title: TorrentTitle | null; recognized: boolean; reasons: string[] }
export interface ImportProposal { service: ServiceKey; items: ImportItem[] }
export interface LibraryTitle { id: number; title: string; detail: string | null; year: number | null }
export type LibraryKey = 'radarr' | 'sonarr' | 'lidarr'
export const LIBRARIES: { key: LibraryKey; label: string }[] = [{ key: 'radarr', label: 'Film' }, { key: 'sonarr', label: 'Série' }, { key: 'lidarr', label: 'Musique' }]

export function depositState(d: Deposit): { label: string; tone: Tone } {
  switch (d.state) {
    case 'checking': return { label: 'Vérification', tone: 'att' }
    case 'seeding': return { label: 'En partage', tone: 'ok' }
    case 'mismatch': return { label: 'Fichiers non conformes', tone: 'dgr' }
    default: return d.received >= d.size ? { label: 'Prêt à vérifier', tone: 'info' as Tone } : { label: 'Fichiers attendus', tone: '' }
  }
}
export const depositComplete = (d: Deposit) => d.files.every((f) => f.received >= f.size)
/** Fichiers à renvoyer après une vérification : ceux que qBittorrent n'a pas trouvés entièrement valides. */
export const invalidFiles = (d: Deposit) => d.files.filter((f) => f.valid !== null && f.valid < 1)

export interface LocalFile { name: string; size: number; path?: string }
const fin = (chemin: string) => chemin.split('/').pop() ?? chemin
/**
 * Associe les fichiers choisis sur l'appareil à ceux du torrent : même taille obligatoire, puis le chemin relatif le plus long en commun,
 * le même nom, et à défaut une taille que seul un fichier de chaque côté porte. Rend, pour chaque index du torrent, la position du fichier local.
 */
export function matchFiles(attendus: DepositFile[], locaux: LocalFile[]): Map<number, number> {
  const pris = new Set<number>()
  const liens = new Map<number, number>()
  const communs = (a: string, b: string) => {
    const [x, y] = [a.split('/').reverse(), b.split('/').reverse()]
    let n = 0
    while (n < x.length && n < y.length && x[n] === y[n]) n++
    return n
  }
  for (const f of attendus) {
    let meilleur = -1
    let score = 0
    locaux.forEach((l, i) => {
      if (pris.has(i) || l.size !== f.size) return
      const s = communs(f.path, l.path || l.name)
      if (s > score) { score = s; meilleur = i }
    })
    if (meilleur >= 0) { liens.set(f.index, meilleur); pris.add(meilleur) }
  }
  for (const f of attendus) {
    if (liens.has(f.index)) continue
    const memes = locaux.map((l, i) => ({ l, i })).filter(({ l, i }) => !pris.has(i) && l.size === f.size)
    const rivaux = attendus.filter((a) => !liens.has(a.index) && a.size === f.size)
    if (memes.length === 1 && rivaux.length === 1) { liens.set(f.index, memes[0].i); pris.add(memes[0].i) }
  }
  return liens
}
export const fileName = fin

export const CHUNK = 16 * 1024 * 1024

export interface SheetVideo { codec: string; resolution: string | null; bit_depth: number | null; dynamic_range: string | null; fps: number | null; bitrate: number | null; scan: string | null }
export interface SheetAudio { codec: string; channels: number | null; languages: string | null; bitrate: number | null; streams: number | null; sample_rate: number | null; bits: number | null }
export interface SheetFile {
  name: string | null
  group: string | null
  size: number | null
  quality: string | null
  release_group: string | null
  scene_name: string | null
  edition: string | null
  languages: string[]
  video: SheetVideo | null
  audio: SheetAudio | null
  subtitles: string | null
  duration: string | null
}
export interface Sheet {
  service: LibraryKey
  id: number
  title: string | null
  original_title: string | null
  artist: string | null
  year: number | null
  runtime: number | null
  genres: string[]
  ids: { tmdb: number | null; imdb: string | null; tvdb: number | null; musicbrainz: string | null }
  files: SheetFile[]
}

const nombre = (n: number, decimales = 0) => n.toLocaleString('fr-FR', { maximumFractionDigits: decimales })
const debit = (bits: number) => (bits >= 1_000_000 ? `${nombre(bits / 1_000_000, 1)} Mb/s` : `${nombre(bits / 1000)} kb/s`)
const canaux = (c: number) => (Number.isInteger(c) ? (c === 1 ? 'mono' : c === 2 ? 'stéréo' : `${c} canaux`) : c.toFixed(1))

export const sheetGroups = (s: Sheet) => [...new Set(s.files.map((f) => f.group).filter((g): g is string => !!g))]
export const sheetTitle = (s: Sheet) => `${s.artist ? `${s.artist} - ` : ''}${s.title ?? 'Sans titre'}${s.year ? ` (${s.year})` : ''}`

function fileLines(f: SheetFile): string[] {
  const lignes = [f.name ?? 'Fichier sans nom']
  const general = [f.size ? `Taille : ${bytes(f.size)}` : '', f.quality ? `Qualité : ${f.quality}` : '', f.release_group ? `Groupe : ${f.release_group}` : '', f.edition ? `Édition : ${f.edition}` : ''].filter(Boolean)
  if (general.length) lignes.push(general.join(' · '))
  if (f.scene_name && f.scene_name !== f.name) lignes.push(`Nom de release : ${f.scene_name}`)
  if (f.video) {
    const v = f.video
    lignes.push(`Vidéo : ${[v.codec, v.resolution, v.bit_depth ? `${v.bit_depth} bits` : '', v.dynamic_range, v.fps ? `${nombre(v.fps, 3)} i/s` : '', v.bitrate ? debit(v.bitrate) : '', v.scan].filter(Boolean).join(', ')}`)
  }
  if (f.audio) {
    const a = f.audio
    lignes.push(`Audio : ${[[a.codec, a.channels ? canaux(a.channels) : ''].filter(Boolean).join(' '), a.languages, a.bitrate ? debit(a.bitrate) : '', a.sample_rate ? `${nombre(a.sample_rate / 1000, 1)} kHz` : '', a.bits ? `${a.bits} bits` : '', a.streams && a.streams > 1 ? `${a.streams} pistes` : ''].filter(Boolean).join(', ')}`)
  }
  if (f.languages.length) lignes.push(`Langues : ${f.languages.join(', ')}`)
  if (f.subtitles) lignes.push(`Sous-titres : ${f.subtitles}`)
  if (f.duration) lignes.push(`Durée : ${f.duration}`)
  return lignes
}

/** Fiche en texte brut, à coller dans le formulaire d'un tracker ; `group` limite aux fichiers d'une saison. */
export function sheetText(s: Sheet, group = ''): string {
  const entete = [sheetTitle(s)]
  if (s.original_title && s.original_title !== s.title) entete.push(`Titre original : ${s.original_title}`)
  const infos = [s.runtime ? `Durée : ${Math.floor(s.runtime / 60)} h ${String(s.runtime % 60).padStart(2, '0')}` : '', s.genres.length ? `Genres : ${s.genres.join(', ')}` : ''].filter(Boolean)
  if (infos.length) entete.push(infos.join(' · '))
  if (s.ids.tmdb) entete.push(`TMDB : https://www.themoviedb.org/${s.service === 'sonarr' ? 'tv' : 'movie'}/${s.ids.tmdb}`)
  if (s.ids.imdb) entete.push(`IMDb : https://www.imdb.com/title/${s.ids.imdb}/`)
  if (s.ids.tvdb) entete.push(`TVDB : https://thetvdb.com/?tab=series&id=${s.ids.tvdb}`)
  if (s.ids.musicbrainz) entete.push(`MusicBrainz : https://musicbrainz.org/release-group/${s.ids.musicbrainz}`)
  const fichiers = s.files.filter((f) => !group || f.group === group)
  const blocs = [entete.join('\n')]
  if (group) blocs.push(group)
  if (!fichiers.length) blocs.push('Aucun fichier.')
  for (const f of fichiers) blocs.push(fileLines(f).join('\n'))
  return blocs.join('\n\n')
}

const get = <T>(p: string) => requestAt<T>(REGIE_URL, 'GET', p)
const post = (p: string, b: unknown) => requestAt<void>(REGIE_URL, 'POST', p, b)

export const regieApi = {
  me: () => get<RegieMe>('/me'),
  status: () => get<RegieStatus>('/admin/status'),
  testIndexer: (id: number) => requestAt<IndexerTest>(REGIE_URL, 'POST', `/admin/indexers/${id}/test`),
  downloads: (since?: number) => get<Downloads>(`/admin/downloads${since ? `?since=${since}` : ''}`),
  pause: (hashes: string[]) => post('/admin/downloads/pause', { hashes }),
  resume: (hashes: string[]) => post('/admin/downloads/resume', { hashes }),
  recheck: (hashes: string[]) => post('/admin/downloads/recheck', { hashes }),
  remove: (hashes: string[], delete_files: boolean) => post('/admin/downloads/delete', { hashes, delete_files }),
  setCategory: (hashes: string[], category: string) => post('/admin/downloads/category', { hashes, category }),
  setTags: (hashes: string[], add: string[], remove: string[]) => post('/admin/downloads/tags', { hashes, add, remove }),
  setLimits: (hashes: string[], download: number, upload: number) => post('/admin/downloads/limits', { hashes, download, upload }),
  setTransfer: (download: number, upload: number, alternative: boolean) => requestAt<void>(REGIE_URL, 'PUT', '/admin/transfer', { download, upload, alternative }),
  torrent: (hash: string) => get<TorrentDetail>(`/admin/downloads/${hash}`),
  setPriority: (hash: string, ids: number[], priority: number) => post(`/admin/downloads/${hash}/files`, { ids, priority }),
  deposits: () => get<Deposits>('/admin/deposits'),
  createDeposit: (torrent: Blob) => requestAt<Deposit>(REGIE_URL, 'POST', '/admin/deposits', torrent.slice(0, torrent.size, 'application/x-bittorrent')),
  deposit: (id: string, wait?: DepositState) => get<Deposit>(`/admin/deposits/${id}${wait ? `?wait=${wait}` : ''}`),
  removeDeposit: (id: string) => requestAt<void>(REGIE_URL, 'DELETE', `/admin/deposits/${id}`),
  sendChunk: (id: string, index: number, offset: number, chunk: Blob, reset = false) => requestAt<{ received: number }>(REGIE_URL, 'PUT', `/admin/deposits/${id}/files/${index}?offset=${offset}${reset ? '&reset=true' : ''}`, chunk),
  seed: (id: string) => requestAt<Deposit>(REGIE_URL, 'POST', `/admin/deposits/${id}/seed`),
  importProposal: (id: string, service: LibraryKey) => get<ImportProposal>(`/admin/deposits/${id}/import?service=${service}`),
  importDeposit: (id: string, service: LibraryKey, movie_id?: number) => requestAt<{ service: ServiceKey; files: number }>(REGIE_URL, 'POST', `/admin/deposits/${id}/import`, movie_id ? { service, movie_id } : { service }),
  library: (service: LibraryKey, q: string) => get<{ items: LibraryTitle[] }>(`/admin/library/${service}?q=${encodeURIComponent(q)}`),
  sheet: (service: LibraryKey, id: number) => get<Sheet>(`/admin/library/${service}/${id}/sheet`),
  removeFromQueue: (service: ServiceKey, id: number, retry: boolean) => requestAt<void>(REGIE_URL, 'DELETE', `/admin/queue/${service}/${id}${retry ? '?retry=true' : ''}`),
}
