export const MAX_FILE_SIZE = 10 * 1024 * 1024
/** Limite des vidéos avant que le serveur ne l'annonce (`max_video_size` de `GET /me`). */
export const DEFAULT_MAX_VIDEO = 100 * 1024 * 1024
const MAX_SIDE = 1920
const COMPRESSIBLE = ['image/jpeg', 'image/png', 'image/webp']

/** Dimensions réduites pour tenir dans un carré de `max` pixels, sans agrandir. */
export function fitWithin(w: number, h: number, max = MAX_SIDE): { w: number; h: number } {
  const k = Math.min(1, max / Math.max(w, h))
  return { w: Math.max(1, Math.round(w * k)), h: Math.max(1, Math.round(h * k)) }
}

/** Remplace l'extension : le serveur refuse une extension d'image qui ne correspond pas au contenu. */
export function swapExt(name: string, ext: string): string {
  const i = name.lastIndexOf('.')
  return (i > 0 ? name.slice(0, i) : name) + '.' + ext
}

/**
 * Compresse une image avant l'envoi (WebP, 1920 px max, qualité 0.82). Les GIF (animations), les autres fichiers
 * et les images déjà légères sont envoyés tels quels ; si le résultat est plus gros, on garde l'original.
 */
export async function prepareUpload(file: File): Promise<{ blob: Blob; name: string }> {
  const keep = { blob: file as Blob, name: file.name }
  if (!COMPRESSIBLE.includes(file.type) || typeof createImageBitmap === 'undefined') return keep
  try {
    const bmp = await createImageBitmap(file)
    const { w, h } = fitWithin(bmp.width, bmp.height)
    if (file.size < 300 * 1024 && w === bmp.width && h === bmp.height) { bmp.close(); return keep }
    const canvas = document.createElement('canvas')
    canvas.width = w
    canvas.height = h
    canvas.getContext('2d')!.drawImage(bmp, 0, 0, w, h)
    bmp.close()
    const out: Blob | null = await new Promise((res) => canvas.toBlob(res, 'image/webp', 0.82))
    if (!out || out.type !== 'image/webp' || out.size >= file.size) return keep
    return { blob: out, name: swapExt(file.name, 'webp') }
  } catch {
    return keep
  }
}

/** Contrôle côté client avant l'envoi ; renvoie un message d'erreur ou null. */
export function checkSize(size: number): string | null {
  return size > MAX_FILE_SIZE ? 'Fichier trop gros (10 Mo maximum)' : null
}

/** Durée maximale d'un message vocal (le serveur refuse au-delà). */
export const VOICE_MAX_MS = 5 * 60 * 1000

/**
 * Format d'enregistrement : Opus dans WebM ou Ogg là où MediaRecorder sait le faire, sinon AAC dans MP4 (Safari).
 * `supported` est injectable pour les tests ; par défaut, MediaRecorder du navigateur.
 */
export function recordingType(supported: (t: string) => boolean = (t) => MediaRecorder.isTypeSupported(t)): { mime: string; ext: string } | null {
  const candidates = [
    { mime: 'audio/webm;codecs=opus', ext: 'webm' },
    { mime: 'audio/ogg;codecs=opus', ext: 'ogg' },
    { mime: 'audio/mp4', ext: 'm4a' },
  ]
  return candidates.find((c) => supported(c.mime)) ?? null
}

export function seekRatio(clientX: number, left: number, width: number): number {
  return width > 0 ? Math.min(1, Math.max(0, (clientX - left) / width)) : 0
}

/** Niveau de 0 à 100 d'un bloc audio, en dBFS : -60 dB ou moins donne 0, 0 dB donne 100. */
export function levelOf(samples: Float32Array): number {
  let sum = 0
  for (const s of samples) sum += s * s
  const db = 10 * Math.log10(sum / Math.max(1, samples.length) + 1e-10)
  return Math.round(Math.min(100, Math.max(0, ((db + 60) * 100) / 60)))
}

/** Réduit des niveaux (0 à 100) à `bars` valeurs : moyenne de chaque tranche. Au moins une valeur. */
export function toWaveform(levels: number[], bars = 40): number[] {
  if (!levels.length) return [0]
  const n = Math.min(bars, levels.length)
  return Array.from({ length: n }, (_, i) => {
    const from = Math.floor((i * levels.length) / n)
    const to = Math.max(from + 1, Math.floor(((i + 1) * levels.length) / n))
    const slice = levels.slice(from, to)
    return Math.round(slice.reduce((a, b) => a + b, 0) / slice.length)
  })
}
