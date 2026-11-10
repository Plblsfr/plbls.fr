export interface FilmProgress { position: number; duration: number; seen: boolean }
export interface FilmItem {
  id: string; kind: 'movie' | 'series' | 'season' | 'episode' | 'other'; title: string; year: number | null; overview: string | null
  duration: number; genres: string[]; season: number | null; episode: number | null; series_id: string | null; series_title: string | null
  poster: boolean; backdrop: boolean; progress: FilmProgress | null
  /** Entracte : fichier présent sur le serveur (un titre vu dont le fichier a disparu reste dans l'historique). */
  available?: boolean
  /** Entracte : date du dernier visionnage complet. */
  seen_at?: string | null
}
export interface FilmTrack { n: number; label: string | null; language: string | null }
export interface FilmDetail extends FilmItem { audio: FilmTrack[]; subtitles: FilmTrack[]; qualities?: Quality[] }
export type Quality = 'original' | '1080p' | '720p' | '480p'
/** Formats que le client sait lire tels quels, annoncés à Entracte (lecture directe ou HLS). */
export interface Support { containers: string[]; video: string[]; audio: string[] }
export interface PlaybackRequest { direct: boolean; audio: number; quality?: Quality; start?: number; support?: Support }
export interface FilmPlayback {
  stream_id: string; mode: 'direct' | 'hls'; url: string; duration: number; audio: FilmTrack[]; subtitles: (FilmTrack & { url: string })[]
  /** Entracte : en HLS, le flux commence à `offset` secondes du titre. */
  offset?: number
  quality?: Quality
  qualities?: Quality[]
}
export interface FilmTracks { audio: FilmTrack[]; subtitles: FilmTrack[]; audioN: number; subN: number | null; qualities?: Quality[]; quality?: Quality }
export interface FilmReject { name: string; reason: string; seen_at: string }

export const SAVE_EVERY_MS = 15_000

export function saveDue(savedAt: number, now: number): boolean {
  return now - savedAt >= SAVE_EVERY_MS
}

const PROBES: { kind: keyof Support; name: string; mime: string }[] = [
  { kind: 'containers', name: 'mp4', mime: 'video/mp4' },
  { kind: 'containers', name: 'webm', mime: 'video/webm' },
  { kind: 'containers', name: 'mkv', mime: 'video/x-matroska' },
  { kind: 'video', name: 'h264', mime: 'video/mp4; codecs="avc1.42E01E"' },
  { kind: 'video', name: 'hevc', mime: 'video/mp4; codecs="hvc1.1.6.L93.B0"' },
  { kind: 'video', name: 'vp9', mime: 'video/webm; codecs="vp9"' },
  { kind: 'video', name: 'av1', mime: 'video/mp4; codecs="av01.0.05M.08"' },
  { kind: 'audio', name: 'aac', mime: 'audio/mp4; codecs="mp4a.40.2"' },
  { kind: 'audio', name: 'mp3', mime: 'audio/mpeg' },
  { kind: 'audio', name: 'opus', mime: 'audio/webm; codecs="opus"' },
  { kind: 'audio', name: 'flac', mime: 'audio/flac' },
  { kind: 'audio', name: 'ac3', mime: 'audio/mp4; codecs="ac-3"' },
  { kind: 'audio', name: 'eac3', mime: 'audio/mp4; codecs="ec-3"' },
]

/** Ce que le lecteur sait lire tel quel (canPlayType), pour qu'Entracte choisisse entre lecture directe et HLS. */
export function support(canPlay: (mime: string) => string): Support {
  const s: Support = { containers: [], video: [], audio: [] }
  for (const p of PROBES) if (canPlay(p.mime) !== '') s[p.kind].push(p.name)
  return s
}

export const QUALITY_LABEL: Record<Quality, string> = { original: 'Originale', '1080p': '1080p', '720p': '720p', '480p': '480p' }
const QUALITY_KEY = 'entracte.qualite'

/** Palier choisi sur cet appareil, repris à la lecture suivante (repli sur « Originale » s'il n'est pas proposé). */
export function savedQuality(store: Pick<Storage, 'getItem'> | null, offered?: Quality[]): Quality {
  let q: string | null = null
  try { q = store?.getItem(QUALITY_KEY) ?? null } catch { /* stockage indisponible */ }
  const ok = (x: string | null): x is Quality => x === 'original' || x === '1080p' || x === '720p' || x === '480p'
  if (!ok(q)) return 'original'
  return !offered || offered.includes(q) ? q : 'original'
}

export function saveQuality(store: Pick<Storage, 'setItem'> | null, q: Quality) {
  try { store?.setItem(QUALITY_KEY, q) } catch { /* stockage indisponible */ }
}

/** Position dans le titre d'après le temps du lecteur (un flux HLS d'Entracte commence à `offset`). */
export const absolute = (t: number, offset: number) => t + offset

/** Faut-il rouvrir le flux pour atteindre `target` ? (HLS : avant le début du flux ou au-delà de ce qui est transcodé) */
export function needsReopen(target: number, offset: number, ready: number): boolean {
  return target < offset - 0.5 || target > offset + ready + 5
}

export function clock(sec: number): string {
  const t = Math.max(0, Math.floor(sec))
  const h = Math.floor(t / 3600)
  const m = Math.floor(t / 60) % 60
  const s = String(t % 60).padStart(2, '0')
  return h ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

export function shortDuration(sec: number): string {
  const min = Math.round(sec / 60)
  if (min < 60) return `${min} min`
  return `${Math.floor(min / 60)} h ${String(min % 60).padStart(2, '0')}`
}

export function remaining(p: FilmProgress): string {
  return `reste ${shortDuration(Math.max(0, p.duration - p.position))}`
}

export const percent = (p: FilmProgress | null) => (p && p.duration > 0 ? `${Math.min(100, (p.position / p.duration) * 100)}%` : '0%')

export function episodeLabel(i: Pick<FilmItem, 'season' | 'episode'>): string {
  return i.season != null && i.episode != null ? `S${i.season} · E${i.episode}` : ''
}

export function fullTitle(i: FilmItem): string {
  const ep = episodeLabel(i)
  return i.series_title && ep ? `${i.series_title} · ${ep}, ${i.title}` : i.title
}

export function resumeAt(i: Pick<FilmItem, 'progress'>): number {
  return i.progress && !i.progress.seen ? i.progress.position : 0
}

export function pickEpisode(episodes: FilmItem[]): FilmItem | undefined {
  return episodes.find((e) => e.progress && !e.progress.seen && e.progress.position > 0) ?? episodes.find((e) => !e.progress?.seen) ?? episodes[0]
}

export const trackName = (t: FilmTrack) => t.label ?? t.language ?? `Piste ${t.n + 1}`

const REASONS: Record<string, string> = {
  extension_interdite: 'Extension interdite',
  double_extension: 'Double extension',
  conteneur_incoherent: 'Conteneur incohérent avec l’extension',
  sans_flux_video: 'Aucun flux vidéo',
  nom_suspect: 'Nom suspect',
}
export const reasonLabel = (code: string) => REASONS[code] ?? code

export function yearOptions(now: number): { value: string; label: string }[] {
  return [
    { value: '', label: 'Toutes' },
    { value: `2020-${Math.max(now, 2020)}`, label: '2020 et après' },
    { value: '2010-2019', label: '2010 à 2019' },
    { value: '2000-2009', label: '2000 à 2009' },
    { value: '1880-1999', label: 'Avant 2000' },
  ]
}

const TINTS = [
  { bg: '#12474f', fg: '#f6efe0', ac: '#e8b04a' },
  { bg: '#4a1f3d', fg: '#f6efe0', ac: '#e07a9a' },
  { bg: '#1d2a55', fg: '#f6efe0', ac: '#f4c7d5' },
  { bg: '#8c3b24', fg: '#fbeedb', ac: '#e9a66b' },
  { bg: '#c98a1b', fg: '#2a1c08', ac: '#f3d27a' },
  { bg: '#222222', fg: '#f2eee7', ac: '#6f8fd1' },
  { bg: '#2c6e7f', fg: '#f6efe0', ac: '#a9d6d0' },
  { bg: '#4d5a2a', fg: '#f4efcf', ac: '#c5c76b' },
]

export function tint(id: string) {
  let h = 0
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0
  return TINTS[h % TINTS.length]
}
