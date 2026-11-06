// Arrière-plan de la caméra (EST-151) : segmentation de la personne sur l'appareil (MediaPipe, modèle servi par l'app, `person.ts`),
// puis flou, fond uni ou image. Rien n'est chargé tant qu'un effet n'est pas choisi : tout passe par import() dynamique.
import type { LocalVideoTrack } from 'livekit-client'
import type { ProcessorWrapper } from '@livekit/track-processors'
import type { PersonOptions, PersonTransformer } from './person'
import { prefs, type CameraFx } from './prefs.svelte'
import { debug } from './debuglog.svelte'

export const FX_LABEL: Record<CameraFx, string> = { none: 'Aucun', blur: 'Flou', cutout: 'Fond supprimé', image: 'Image' }

/** Fonds proposés par défaut, dessinés à la demande (aucune image à embarquer). */
export const BACKGROUNDS = [
  { id: 'aube', label: 'Aube', colors: ['#f6c8a8', '#e0796b'] },
  { id: 'nuit', label: 'Nuit', colors: ['#2b2f4f', '#6b4a7a'] },
  { id: 'foret', label: 'Forêt', colors: ['#bcd9b0', '#4f7a5c'] },
]
// Couleurs d'Estompe : une caméra ne transporte pas la transparence, la personne détourée est posée sur ce fond.
const NEUTRAL = ['#faf6f3', '#eadbd2']

const BASE = import.meta.env.BASE_URL + 'mediapipe/'
const ASSETS = { tasksVisionFileSet: BASE.replace(/\/$/, ''), modelAssetPath: BASE + 'selfie_segmenter_landscape.tflite' }
/** Au-delà de ce temps moyen par image (ms), la machine ne suit pas : l'effet est coupé. */
const SLOW_MS = 40

function gradient(colors: string[]): string {
  const cv = document.createElement('canvas')
  cv.width = 1280
  cv.height = 720
  const ctx = cv.getContext('2d')!
  const g = ctx.createLinearGradient(0, 0, 1280, 720)
  colors.forEach((c, i) => g.addColorStop(i / (colors.length - 1), c))
  ctx.fillStyle = g
  ctx.fillRect(0, 0, 1280, 720)
  return cv.toDataURL('image/jpeg', 0.85)
}

/** Image de fond pour « Fond supprimé » et « Image » ; vide si l'image perso manque. */
export function backgroundUrl(fx: CameraFx = prefs.cameraFx): string {
  if (fx === 'cutout') return gradient(NEUTRAL)
  if (prefs.cameraBg === 'custom') return prefs.customBg
  return gradient((BACKGROUNDS.find((b) => b.id === prefs.cameraBg) ?? BACKGROUNDS[0]).colors)
}

/** Redimensionne (recadrage 16:9) et encode une image PNG, JPEG ou WebP choisie par la personne. */
export async function prepareBackground(file: Blob): Promise<Blob> {
  if (!/^image\/(png|jpeg|webp)$/.test(file.type)) throw new Error('Image PNG, JPEG ou WebP attendue.')
  const bmp = await createImageBitmap(file)
  const cv = document.createElement('canvas')
  cv.width = 1280
  cv.height = 720
  const k = Math.max(1280 / bmp.width, 720 / bmp.height)
  cv.getContext('2d')!.drawImage(bmp, (1280 - bmp.width * k) / 2, (720 - bmp.height * k) / 2, bmp.width * k, bmp.height * k)
  bmp.close()
  return new Promise((res, rej) => cv.toBlob((b) => (b ? res(b) : rej(new Error('Image illisible.'))), 'image/jpeg', 0.85))
}

type Wrapper = ProcessorWrapper<PersonOptions, PersonTransformer>

function optionsFor(fx: Exclude<CameraFx, 'none'>): Partial<PersonOptions> {
  return fx === 'blur' ? { blurRadius: prefs.cameraBlur, imagePath: undefined } : { imagePath: backgroundUrl(fx) }
}

/**
 * Brique réutilisable (incrustation du partage d'écran, EST-150) : le processeur de segmentation.
 * `transformer.mask` donne le masque lissé de la personne (0 à 1) à chaque image.
 */
export async function createPersonProcessor(fx: Exclude<CameraFx, 'none'>, onSlow?: () => void): Promise<Wrapper> {
  const [{ ProcessorWrapper }, { PersonTransformer }] = await Promise.all([import('@livekit/track-processors'), import('./person')])
  let n = 0
  let avg = 0
  const onFrameProcessed = ({ processingTimeMs }: { processingTimeMs: number }) => {
    avg = n ? avg * 0.95 + processingTimeMs * 0.05 : processingTimeMs
    if (++n % 150 === 0) debug.log('info', 'caméra', `effet ${fx} : ${avg.toFixed(1)} ms par image`)
    if (n === 60 && avg > SLOW_MS) {
      debug.log('warn', 'caméra', `effet ${fx} coupé : ${avg.toFixed(1)} ms par image (> ${SLOW_MS} ms)`)
      onSlow?.()
    }
  }
  return new ProcessorWrapper(new PersonTransformer({ assetPaths: ASSETS, onFrameProcessed, ...optionsFor(fx) }), 'estompe-arriere-plan')
}

/** Masque (alpha) de la personne de la caméra publiée quand « Fond supprimé » est actif ; sinon rien. */
export function cameraMatte(track: LocalVideoTrack): OffscreenCanvas | undefined {
  const cur = procs.get(track)
  return cur?.fx === 'cutout' ? cur.proc.transformer.alpha : undefined
}

const procs = new WeakMap<LocalVideoTrack, { fx: CameraFx; proc: Wrapper }>()

/** Met la caméra en accord avec les préférences : « Aucun » retire tout, sans rien charger. Renvoie faux si l'effet n'a pas pu démarrer. */
export async function applyCameraFx(track: LocalVideoTrack | undefined, onSlow?: () => void): Promise<boolean> {
  if (!track) return true
  const fx = prefs.cameraFx
  const cur = procs.get(track)
  if (fx === 'none') {
    if (cur) {
      procs.delete(track)
      await track.stopProcessor()
    }
    return true
  }
  if (fx === 'image' && !backgroundUrl(fx)) return true
  try {
    if (cur && cur.fx !== 'none') {
      // Même type d'effet : mise à jour à la volée, sans relancer le modèle.
      await cur.proc.updateTransformerOptions(optionsFor(fx))
      procs.set(track, { fx, proc: cur.proc })
      return true
    }
    const proc = await createPersonProcessor(fx, onSlow)
    await track.setProcessor(proc)
    procs.set(track, { fx, proc })
    debug.log('info', 'caméra', `effet ${fx} activé`)
    return true
  } catch (e) {
    debug.log('error', 'caméra', `effet ${fx} impossible : ${e}`)
    procs.delete(track)
    await track.stopProcessor().catch(() => {})
    return false
  }
}
