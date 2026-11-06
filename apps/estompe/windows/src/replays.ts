// Bibliothèque des replays de Bobyn (EST-127) : appels à l'app Windows et calculs purs de la vue (tri, recadrage, estimation de taille).
/** Marqueur posé pendant l'enregistrement (EST-130) : `t` en secondes depuis le début du replay. */
export interface Marker { t: number; name: string }
export interface Replay { name: string; game: string; size: number; modified: number; duration: number; width: number; height: number; fps: number; markers: Marker[]; tracks: string[] }
export interface Library { dir: string; replays: Replay[] }
export interface Crop { x: number; y: number; w: number; h: number }
export type Quality = 'original' | '1080' | '720'
/** `target_kbps` : débit vidéo imposé au réencodage (envoi d'un replay trop lourd), au lieu de celui de la source. */
export interface ExportParams { name: string; start: number; end: number; crop: Crop | null; quality: Quality; target_kbps?: number; audio?: AudioMix; encoder?: string }
export interface ExportEvent { id: number; state: 'run' | 'done' | 'error' | 'cancel'; pct: number; file: string | null; mode: 'copy' | 'encode' | null; message: string | null }
export interface AudioMix { tracks: { index: number; volume: number }[]; mixed: boolean }
/** Une piste audio réglable d'un replay : `on` = gardée (et audible dans le lecteur), `volume` de 0 à 1. */
export interface AudioTrack { index: number; name: string; volume: number; on: boolean }
export interface ExportPlan { copy: boolean; reason: string | null }
export type Sort = 'date' | 'duration' | 'size'

const core = () => import('@tauri-apps/api/core')
export const replaysList = async () => (await core()).invoke<Library>('replays_list')
export const replaysThumb = async (name: string) => (await core()).convertFileSrc(await (await core()).invoke<string>('replays_thumb', { name }))
export const replaysRename = async (name: string, to: string) => (await core()).invoke<string>('replays_rename', { name, to })
export const replaysMarkersSet = async (name: string, markers: Marker[]) => (await core()).invoke('replays_markers_set', { name, markers })
export const replaysDelete = async (name: string) => (await core()).invoke('replays_delete', { name })
export const replaysReveal = async (name: string | null) => (await core()).invoke('replays_reveal', { name })
export const replaysCopy = async (name: string) => (await core()).invoke('replays_copy', { name })
export const replaysExport = async (params: ExportParams) => (await core()).invoke<number>('replays_export', { params })
export const replaysPlan = async (params: ExportParams) => (await core()).invoke<ExportPlan>('replays_plan', { params })
export const replaysKeyframes = async (name: string) => (await core()).invoke<number[]>('replays_keyframes', { name })
export const replaysEncoders = async () => (await core()).invoke<string[]>('replays_encoders')
export const replaysCancel = async (id: number) => (await core()).invoke('replays_cancel', { id })
export const replaysTracks = async (name: string) => { const c = await core(); return (await c.invoke<string[]>('replays_tracks', { name })).map((p) => c.convertFileSrc(p)) }
export const replayUrl = async (dir: string, name: string) => (await core()).convertFileSrc(`${dir}\\${name}`)
export const onExport = async (fn: (e: ExportEvent) => void) => (await import('@tauri-apps/api/event')).listen<ExportEvent>('replays-export', (e) => fn(e.payload))

export const formatDuration = (s: number) => {
  const t = Math.max(0, Math.round(s))
  const [h, m, sec] = [Math.floor(t / 3600), Math.floor((t % 3600) / 60), t % 60]
  return `${h ? `${h}:${String(m).padStart(2, '0')}` : m}:${String(sec).padStart(2, '0')}`
}

/** Durée d'une image (60 i/s si la fréquence est inconnue). */
export const frameDuration = (fps: number) => 1 / (fps > 0 ? fps : 60)

/** Timecode précis `m:ss.d` (dixièmes), pour la saisie des bornes de découpe. */
export const formatTimecode = (s: number) => {
  const t = Math.max(0, Math.round(s * 10) / 10)
  const [h, m, sec] = [Math.floor(t / 3600), Math.floor((t % 3600) / 60), t - Math.floor(t / 60) * 60]
  return `${h ? `${h}:${String(m).padStart(2, '0')}` : m}:${sec.toFixed(1).padStart(4, '0')}`
}

/** `90`, `1:30`, `1:30.5` ou `1:02:03` → secondes ; null si illisible. */
export function parseTimecode(text: string): number | null {
  const parts = text.trim().replace(',', '.').split(':')
  if (parts.length > 3 || parts.some((p) => !/^\d+(\.\d+)?$/.test(p))) return null
  return parts.reduce((n, p) => n * 60 + Number(p), 0)
}

/** Image clé la plus proche de `t` à moins de `within` secondes, sinon `t`. */
export function snapToKeyframe(t: number, keyframes: number[], within: number): number {
  let best = t
  let gap = within
  for (const k of keyframes) if (Math.abs(k - t) <= gap) { best = k; gap = Math.abs(k - t) }
  return best
}

/** Zone de recadrage ramenée dans l'image, dimensions et position paires (exigence de l'encodeur), taille minimale 32. */
export function clampCrop(c: Crop, w: number, h: number): Crop {
  const cw = Math.min(w, Math.max(32, Math.round(c.w / 2) * 2))
  const ch = Math.min(h, Math.max(32, Math.round(c.h / 2) * 2))
  return { x: Math.min(w - cw, Math.max(0, Math.round(c.x / 2) * 2)), y: Math.min(h - ch, Math.max(0, Math.round(c.y / 2) * 2)), w: cw, h: ch }
}

/** Zone après déplacement d'une poignée de coin (`nw`, `ne`, `sw`, `se`) au point (px, py) de l'image ; le coin opposé reste fixe. */
export function resizeCrop(c: Crop, corner: 'nw' | 'ne' | 'sw' | 'se', px: number, py: number, w: number, h: number): Crop {
  const [fx, fy] = [corner.includes('w') ? c.x + c.w : c.x, corner.includes('n') ? c.y + c.h : c.y]
  const [x, y] = [Math.min(w, Math.max(0, px)), Math.min(h, Math.max(0, py))]
  return clampCrop({ x: Math.min(x, fx), y: Math.min(y, fy), w: Math.abs(x - fx), h: Math.abs(y - fy) }, w, h)
}

/** Libellé de l'indicateur d'export. */
export const planLabel = (p: ExportPlan) => (p.copy ? 'Copie sans réencodage (instantané, sans perte)' : `Réencodage : ${p.reason}`)

export const formatSize = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} Go` : `${Math.max(1, Math.round(b / 1e6))} Mo`)

/** Recherche par nom, filtre par jeu, tri décroissant sur le critère choisi. */
export function visible(list: Replay[], query: string, game: string, sort: Sort): Replay[] {
  const q = query.trim().toLowerCase()
  const key = { date: (r: Replay) => r.modified, duration: (r: Replay) => r.duration, size: (r: Replay) => r.size }[sort]
  return list.filter((r) => (!game || r.game === game) && (!q || r.name.toLowerCase().includes(q))).sort((a, b) => key(b) - key(a) || a.name.localeCompare(b.name))
}

/** Zone de recadrage du ratio `ratio` (largeur / hauteur) centrée dans une image `w` × `h`, en pixels pairs ; null = libre. */
const MIX = 'Mix'

/** Pistes réglables d'un replay : toutes sauf la piste « Mix » (rendue par le lecteur), et seulement s'il y en a au moins deux. */
export function audioTracks(r: Pick<Replay, 'tracks'>): AudioTrack[] {
  const t = (r.tracks ?? []).map((name, index) => ({ index, name, volume: 1, on: true })).filter((x) => x.name !== MIX)
  return t.length > 1 ? t : []
}

/** Volume réel d'une piste dans le lecteur (le volume général du lecteur s'applique par-dessus). */
export const trackVolume = (t: AudioTrack, master: number) => (t.on ? Math.min(1, t.volume * master) : 0)

/** Demande d'export : les pistes gardées avec leur volume. Rien si le son reste tel quel (la piste « Mix » du fichier suffit). */
export function audioMix(tracks: AudioTrack[], mixed: boolean): AudioMix | undefined {
  if (!tracks.length || (mixed && tracks.every((t) => t.on && t.volume === 1))) return undefined
  return { tracks: tracks.filter((t) => t.on).map((t) => ({ index: t.index, volume: t.volume })), mixed }
}

export function cropForRatio(ratio: number | null, w: number, h: number): Crop | null {
  if (!ratio) return null
  const [cw, ch] = w / h > ratio ? [h * ratio, h] : [w, w / ratio]
  const [pw, ph] = [Math.floor(cw / 2) * 2, Math.floor(ch / 2) * 2]
  return { x: Math.floor((w - pw) / 4) * 2, y: Math.floor((h - ph) / 4) * 2, w: pw, h: ph }
}

/** Taille estimée du fichier exporté : proportionnelle à la durée, et aux pixels en cas de réencodage (même règle que le débit côté Rust). */
export function estimateSize(r: Replay, start: number, end: number, crop: Crop | null, quality: Quality): number {
  const share = r.duration > 0 ? Math.max(0, end - start) / r.duration : 1
  if (!crop && quality === 'original') return r.size * share
  const max = quality === 'original' ? 0 : +quality
  const h = crop?.h ?? r.height
  const outH = max && h > max ? max : h
  return r.size * share * Math.max(0.25, Math.min(1, (outH / (r.height || outH)) ** 2))
}

// --- Marqueurs (EST-130) ---

/** Plage d'un extrait autour de l'instant `t` (`before` s avant, `after` s après), bornée à la durée du replay. */
export function clipAround(t: number, before: number, after: number, duration: number): [number, number] {
  return [Math.max(0, t - Math.max(0, before)), Math.min(duration, t + Math.max(0, after))]
}

/** Marqueurs triés dans le temps, avec celui de `m` ajouté, renommé (`name`) ou retiré (`name === null`). */
export function withMarker(list: Marker[], m: Marker, name?: string | null): Marker[] {
  const rest = list.filter((x) => x !== m)
  const next = name === null ? rest : [...rest, name === undefined ? m : { ...m, name: name.trim() || m.name }]
  return next.sort((a, b) => a.t - b.t)
}

/** Nom d'un marqueur ajouté depuis le lecteur : le premier « Marqueur N » libre. */
export function markerName(list: Marker[]): string {
  let n = list.length + 1
  while (list.some((m) => m.name === `Marqueur ${n}`)) n++
  return `Marqueur ${n}`
}

/** Libellé du nombre de marqueurs sur la miniature ; vide s'il n'y en a pas. */
export const markerCount = (r: Pick<Replay, 'markers'>) => (r.markers?.length ? `${r.markers.length} marqueur${r.markers.length > 1 ? 's' : ''}` : '')

// --- Envoi d'un replay dans une conversation (EST-128) ---

/** Débit audio (kb/s) du réencodage pour l'envoi. */
export const SEND_AUDIO_KBPS = 128

/** Un replay plus gros que la limite vidéo du serveur ne peut partir que réencodé. */
export const needsCompression = (size: number, limit: number) => size > limit

/** Débit vidéo (kb/s) pour qu'un replay de `seconds` tienne à 90 % de `limit` octets, audio compris ; jamais sous 500 kb/s. */
export function targetKbps(limit: number, seconds: number): number {
  return Math.max(500, Math.floor((limit * 0.9 * 8) / 1000 / Math.max(1, seconds)) - SEND_AUDIO_KBPS)
}

/** Taille estimée d'un replay réencodé à `kbps` (vidéo) : ne dépasse jamais l'original, qu'on enverrait alors tel quel. */
export const compressedSize = (r: Pick<Replay, 'duration' | 'size'>, kbps: number) => Math.min(r.size, Math.round(((kbps + SEND_AUDIO_KBPS) * 1000 * r.duration) / 8))

/** Réseau coupé, serveur en panne ou saturé : l'envoi est retenté ; une réponse refusée (droits, limite, type) ne l'est pas. */
export const isTransient = (e: unknown) => !!e && typeof e === 'object' && 'status' in e && ((e.status as number) === 0 || (e.status as number) >= 500 || ((e.status as number) === 429 && (e as { code?: string }).code === 'too_many_video_uploads'))

/** Exécute `fn` jusqu'à `waits.length + 1` fois tant que l'erreur est transitoire ; `wait` est injectable pour les tests. */
export async function withRetry<T>(fn: (attempt: number) => Promise<T>, waits = [2000, 5000, 15000, 30000], wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms)), onRetry?: (attempt: number) => void): Promise<T> {
  for (let i = 0; ; i++) {
    try {
      return await fn(i)
    } catch (e) {
      if (i >= waits.length || !isTransient(e)) throw e
      onRetry?.(i + 1)
      await wait(waits[i])
    }
  }
}

export interface Destination { key: string; label: string; group: string }

/** Destinations possibles : salons texte où l'on peut écrire et joindre des fichiers, puis messages privés. `can` : droit « joindre des fichiers » sur la conversation. */
export function destinations(
  servers: { id: string; name: string }[],
  channels: Record<string, { id: string; name: string; kind: string }[]>,
  dms: { id: string; other: { pseudo: string } }[],
  can: (key: string) => boolean,
): Destination[] {
  const out: Destination[] = []
  for (const s of servers) {
    for (const c of channels[s.id] ?? []) if (c.kind === 'text' && can(`c:${c.id}`)) out.push({ key: `c:${c.id}`, label: `# ${c.name}`, group: s.name })
  }
  for (const d of dms) out.push({ key: `d:${d.id}`, label: d.other.pseudo, group: 'Messages privés' })
  return out
}

/** Recherche sur le salon, le serveur ou la personne. */
export function filterDestinations(list: Destination[], query: string): Destination[] {
  const q = query.trim().toLowerCase()
  return q ? list.filter((d) => `${d.label} ${d.group}`.toLowerCase().includes(q)) : list
}

/** Message d'erreur d'un envoi de replay : limites propres aux vidéos, sinon le message du serveur. */
export function sendErrorMessage(e: unknown): string {
  const x = e as { status?: number; code?: string; message?: string }
  if (x?.status === 413 || x?.code === 'unsupported_video' || x?.code === 'slowmode' || x?.code === 'quota_exceeded') return x.message || 'Envoi refusé.'
  if (x?.status === 403) return "Vous ne pouvez pas joindre de fichiers ici."
  if (x?.status === 0) return 'Connexion au serveur impossible.'
  return x?.message || 'Envoi impossible.'
}
