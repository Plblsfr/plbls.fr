import type { Reseau } from '../../lib/reseau'
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
export interface FilmTrack { n: number; label: string | null; language: string | null; forced?: boolean }
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
  intro?: Intro | null
  /** Entracte : codec de la source et vidéo recopiée sans réencodage. */
  video?: { codec: string; copy: boolean }
  /** Entracte : deux flux de même clé ont les mêmes segments (même fichier, qualité, piste audio, découpage). */
  cache?: string | null
}
export interface Intro { debut: number; fin: number }
export interface FilmTracks { audio: FilmTrack[]; subtitles: FilmTrack[]; audioN: number; subN: number | null; qualities?: Quality[]; quality?: Quality; intro?: Intro | null }
export const inIntro = (position: number, intro: Intro | null | undefined): intro is Intro => !!intro && intro.fin > intro.debut && position >= intro.debut && position < intro.fin
/** Diffusion sur un autre appareil (Chromecast, AirPlay) par le navigateur. */
export type CastState = 'disconnected' | 'connecting' | 'connected'

const CAST_REFUSALS: Record<string, string> = {
  NotFoundError: 'Aucune télé ni appareil de diffusion trouvé sur ce réseau.',
  NotSupportedError: 'Ce titre ne peut pas être diffusé depuis ce navigateur.',
  InvalidAccessError: 'Appuyez de nouveau sur « Diffuser » pour choisir l’appareil.',
}

/** Message à afficher quand le navigateur refuse d'ouvrir le choix de l'appareil (rien si la personne a seulement fermé le choix). */
export const castRefusal = (name: string): string | null => CAST_REFUSALS[name] ?? null

export interface FilmReject { name: string; reason: string; seen_at: string }

export const SAVE_EVERY_MS = 15_000
/** Sans geste pendant ce temps, une lecture en cours masque ses commandes. */
export const IDLE_MS = 2500

export function saveDue(savedAt: number, now: number): boolean {
  return now - savedAt >= SAVE_EVERY_MS
}

const PROBES: { kind: keyof Support; name: string; mime: string }[] = [
  { kind: 'containers', name: 'mp4', mime: 'video/mp4' },
  { kind: 'containers', name: 'webm', mime: 'video/webm' },
  { kind: 'containers', name: 'mkv', mime: 'video/x-matroska' },
  { kind: 'video', name: 'h264', mime: 'video/mp4; codecs="avc1.42E01E"' },
  { kind: 'video', name: 'hevc', mime: 'video/mp4; codecs="hvc1.1.6.L93.B0"' },
  { kind: 'video', name: 'hevc10', mime: 'video/mp4; codecs="hvc1.2.4.L153.B0"' },
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
export function support(canPlay: (mime: string) => string, refused: string[] = []): Support {
  const s: Support = { containers: [], video: [], audio: [] }
  for (const p of PROBES) {
    if (p.kind === 'video' && refused.some((r) => p.name.startsWith(r))) continue
    if (canPlay(p.mime) !== '') s[p.kind].push(p.name)
  }
  return s
}

const REFUSED_KEY = 'entracte.refus'

/** Codecs vidéo que cet appareil annonce mais n'a pas su lire : plus jamais demandés tels quels. */
export function refusedCodecs(store: Pick<Storage, 'getItem'> | null): string[] {
  try {
    const v: unknown = JSON.parse(store?.getItem(REFUSED_KEY) ?? '[]')
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []
  } catch { return [] }
}

export function refuseCodec(store: Pick<Storage, 'getItem' | 'setItem'> | null, codec: string) {
  try { store?.setItem(REFUSED_KEY, JSON.stringify([...new Set([...refusedCodecs(store), codec])])) } catch { return }
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

export type AudioPref = 'auto' | 'vf' | 'vff' | 'vfq' | 'en'
export type SubtitlePref = 'none' | 'forced' | 'fr' | 'en'
export type BufferPref = 'normal' | 'long' | 'all'
export interface PlayPrefs { audio: AudioPref; subtitles: SubtitlePref; volume: number; muted: boolean; buffer: BufferPref; wifiOnly: boolean }
export const BUFFER_PREFS: [BufferPref, string][] = [['normal', 'Normal (1 à 3 min)'], ['long', 'Long (15 min)'], ['all', 'Tout le film']]
export const AUDIO_PREFS: [AudioPref, string][] = [['auto', 'Piste du fichier'], ['vf', 'Français (VF)'], ['vff', 'Français de France (VFF)'], ['vfq', 'Français du Québec (VFQ)'], ['en', 'Anglais']]
export const SUBTITLE_PREFS: [SubtitlePref, string][] = [['none', 'Aucun'], ['forced', 'Forcés seulement'], ['fr', 'Français'], ['en', 'Anglais']]
const PLAY_KEY = 'entracte.lecture'
const PLAY_DEFAULTS: PlayPrefs = { audio: 'auto', subtitles: 'none', volume: 100, muted: false, buffer: 'normal', wifiOnly: false }

export function playPrefs(store: Pick<Storage, 'getItem'> | null): PlayPrefs {
  let p: Partial<PlayPrefs> = {}
  try { const v: unknown = JSON.parse(store?.getItem(PLAY_KEY) ?? '{}'); if (v && typeof v === 'object') p = v } catch { p = {} }
  const volume = typeof p.volume === 'number' && Number.isFinite(p.volume) ? Math.min(100, Math.max(0, Math.round(p.volume))) : PLAY_DEFAULTS.volume
  return {
    audio: AUDIO_PREFS.some(([id]) => id === p.audio) ? p.audio! : PLAY_DEFAULTS.audio,
    subtitles: SUBTITLE_PREFS.some(([id]) => id === p.subtitles) ? p.subtitles! : PLAY_DEFAULTS.subtitles,
    volume, muted: p.muted === true,
    buffer: BUFFER_PREFS.some(([id]) => id === p.buffer) ? p.buffer! : PLAY_DEFAULTS.buffer,
    wifiOnly: p.wifiOnly === true,
  }
}

/** Réglage appliqué : « Wi-Fi seulement » ramène au réglage normal en données mobiles. */
export function effectiveBuffer(p: Pick<PlayPrefs, 'buffer' | 'wifiOnly'>, reseau: Reseau): BufferPref {
  return p.wifiOnly && reseau === 'cellular' ? 'normal' : p.buffer
}

export function savePlayPrefs(store: Pick<Storage, 'getItem' | 'setItem'> | null, patch: Partial<PlayPrefs>) {
  try { store?.setItem(PLAY_KEY, JSON.stringify({ ...playPrefs(store), ...patch })) } catch { return }
}

const text = (t: FilmTrack) => `${t.label ?? ''} ${t.language ?? ''}`
const FAMILIES: [string, RegExp, RegExp][] = [
  ['fr', /^(fr|fre|fra)(-|$)/i, /\b(vf[fqi2]?|vostfr|truefrench|french|fran[cç]ais)/i],
  ['en', /^(en|eng)(-|$)/i, /\b(english|anglais)/i],
]

export function trackFamily(t: FilmTrack): string | null {
  const known = FAMILIES.find(([, code, word]) => (t.language ? code.test(t.language) : word.test(t.label ?? '')))
  return known ? known[0] : t.language ? t.language.toLowerCase().split('-')[0] : null
}

export const isForced = (t: FilmTrack) => t.forced === true || /forc/i.test(t.label ?? '')

const VFF = /\bvff\b|truefrench|\bfrance\b|\bfr-fr\b/i
const VFQ = /\bvfq\b|qu[ée]b|canad|\bfr-ca\b/i
const ASIDE = /comment|audio ?desc|descripti/i
const IMPAIRED = /\bsdh\b|malentendant|\bcc\b/i

const best = <T extends FilmTrack>(tracks: T[], score: (t: T) => number): T | undefined => {
  let top: T | undefined
  let max = 0
  for (const t of tracks) { const s = score(t); if (s > max) { max = s; top = t } }
  return top
}

export function pickAudio(tracks: FilmTrack[], pref: AudioPref): number {
  if (pref === 'auto') return 0
  const family = pref === 'en' ? 'en' : 'fr'
  const wanted = pref === 'vff' ? VFF : pref === 'vfq' ? VFQ : null
  const other = pref === 'vff' ? VFQ : pref === 'vfq' ? VFF : null
  return best(tracks, (t) => {
    if (trackFamily(t) !== family) return 0
    const variant = wanted?.test(text(t)) ? 2 : other?.test(text(t)) ? 0 : 1
    return 1 + variant + (ASIDE.test(t.label ?? '') ? 0 : 3)
  })?.n ?? 0
}

export function pickSubtitle(tracks: FilmTrack[], pref: SubtitlePref, audio?: FilmTrack): number | null {
  if (pref === 'none') return null
  if (pref === 'forced') {
    const family = audio ? trackFamily(audio) : null
    return best(tracks, (t) => (isForced(t) && (!family || trackFamily(t) === family) ? 1 : 0))?.n ?? null
  }
  return best(tracks, (t) => (trackFamily(t) !== pref ? 0 : 1 + (isForced(t) ? 0 : 2) + (IMPAIRED.test(t.label ?? '') ? 0 : 1)))?.n ?? null
}

export type Shortcut = 'toggle' | 'back' | 'forward' | 'louder' | 'quieter' | 'mute' | 'fullscreen' | 'skip'
export const SEEK_STEP = 10
export const VOLUME_STEP = 5
const SHORTCUTS: Record<string, Shortcut> = {
  ' ': 'toggle', k: 'toggle', ArrowLeft: 'back', j: 'back', ArrowRight: 'forward', l: 'forward', ArrowUp: 'louder', ArrowDown: 'quieter', m: 'mute', f: 'fullscreen', s: 'skip',
}

export function shortcut(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey'>): Shortcut | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null
  return SHORTCUTS[e.key.length === 1 ? e.key.toLowerCase() : e.key] ?? null
}

/** Position dans le titre d'après le temps du lecteur (un flux HLS d'Entracte commence à `offset`). */
export const absolute = (t: number, offset: number) => t + offset

/** Ramène les répliques d'une piste (horaires du titre) au temps du lecteur, dont le flux HLS commence à `offset`. */
export function shiftCues(cues: ArrayLike<{ startTime: number; endTime: number }> | null | undefined, offset: number) {
  if (!cues || !offset) return
  for (let i = 0; i < cues.length; i++) { cues[i].startTime -= offset; cues[i].endTime -= offset }
}

/** Faut-il rouvrir le flux pour atteindre `target` ? (HLS : avant le début du flux ou au-delà de ce qui est transcodé) */
export function needsReopen(target: number, offset: number, ready: number): boolean {
  return target < offset - 0.5 || target > offset + ready + 5
}

export type Range = [number, number]

const PISTE_LUE = 'var(--rose-fonce)'
const PISTE_CHARGEE = 'rgb(255 255 255 / 52%)'
const PISTE_VIDE = 'rgb(255 255 255 / 22%)'

export function mergeRanges(ranges: Range[], gap = 0.5): Range[] {
  const out: Range[] = []
  for (const [a, b] of ranges.filter(([a, b]) => Number.isFinite(a) && Number.isFinite(b) && b > a).sort((x, y) => x[0] - y[0])) {
    const last = out[out.length - 1]
    if (last && a <= last[1] + gap) last[1] = Math.max(last[1], b)
    else out.push([a, b])
  }
  return out
}

export function seekTrack(position: number, duration: number, ranges: Range[]): string {
  if (!(duration > 0)) return `linear-gradient(to right, ${PISTE_VIDE} 0 100%)`
  const pct = (s: number) => `${Math.round(Math.min(1, Math.max(0, s / duration)) * 10000) / 100}%`
  const lu = Math.min(duration, Math.max(0, position))
  const stops = [`${PISTE_LUE} 0 ${pct(lu)}`]
  let at = lu
  for (const [a, b] of mergeRanges(ranges)) {
    const from = Math.max(a, at)
    const to = Math.min(b, duration)
    if (to <= from) continue
    if (from > at) stops.push(`${PISTE_VIDE} ${pct(at)} ${pct(from)}`)
    stops.push(`${PISTE_CHARGEE} ${pct(from)} ${pct(to)}`)
    at = to
  }
  if (at < duration) stops.push(`${PISTE_VIDE} ${pct(at)} 100%`)
  return `linear-gradient(to right, ${stops.join(', ')})`
}

export function bufferedAhead(position: number, ranges: Range[]): number {
  const r = mergeRanges(ranges).find(([a, b]) => position >= a - 0.5 && position < b)
  return r ? Math.max(0, r[1] - position) : 0
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

export const NEXT_BEFORE_END = 30

export const nearEnd = (position: number, duration: number) => duration > 0 && position >= duration - NEXT_BEFORE_END

export function episodeAfter(episodes: FilmItem[], id: string): FilmItem | undefined {
  const i = episodes.findIndex((e) => e.id === id)
  return i < 0 ? undefined : episodes.slice(i + 1).find((e) => e.available !== false)
}

export function seasonAfter(seasons: FilmItem[], n: number): FilmItem | undefined {
  return seasons.filter((s) => s.season != null && s.season > n && s.available !== false).sort((x, y) => x.season! - y.season!)[0]
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
