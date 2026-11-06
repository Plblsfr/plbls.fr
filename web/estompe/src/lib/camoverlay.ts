// Incrustation de la caméra dans le partage d'écran (EST-150, Alpha) : la caméra est composée sur chaque image du partage
// avant l'encodage, les spectateurs voient donc le résultat sans traitement de leur côté. Rien de ce fichier n'est chargé
// tant que l'interrupteur est coupé : voice.svelte.ts et les Paramètres passent par import() dynamique.
import type { LocalVideoTrack } from 'livekit-client'
import type { VideoTrackTransformer, VideoTransformerInitOptions } from '@livekit/track-processors'
import { cameraMatte } from './camerafx'
import { debug } from './debuglog.svelte'
import { prefs, type CamOverlay } from './prefs.svelte'

type Ctx = OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D

/** Rectangle de l'incrustation dans une image w × h ; `ratio` = largeur / hauteur de la caméra (carré pour le cercle). */
export function overlayRect(o: Pick<CamOverlay, 'x' | 'y' | 'size' | 'shape' | 'margin'>, w: number, h: number, ratio: number) {
  const rw = Math.round(o.size * w)
  const rh = Math.round(o.shape === 'circle' ? rw : rw / ratio)
  const m = Math.min(o.margin, Math.max(0, (w - rw) / 2), Math.max(0, (h - rh) / 2))
  return { x: Math.round(m + o.x * (w - rw - 2 * m)), y: Math.round(m + o.y * (h - rh - 2 * m)), w: rw, h: rh }
}

/** Paliers de la réduction automatique : part de la taille de l'incrustation et délai minimal entre deux rendus de la caméra (ms ; le premier laisse de la marge sous une image à 60/s pour ne pas en sauter une sur deux). */
const LEVELS = [{ scale: 1, every: 12 }, { scale: 0.7, every: 40 }, { scale: 0.5, every: 90 }]
/** Au-delà de ce temps moyen par rendu de la caméra (ms), l'incrustation descend d'un palier. */
const BUDGET_MS = 8
const WINDOW = 30

/** Rend la caméra (forme, bordure, miroir) dans un calque réutilisé à chaque image du partage. */
export class Composer {
  level = 0
  private layer?: OffscreenCanvas
  private last = -Infinity
  private n = 0
  private avg = 0

  constructor(
    private video: HTMLVideoElement,
    private matte?: () => CanvasImageSource | undefined,
    private onSlow?: (msg: string) => void,
  ) {}

  /** Temps d'un rendu : une fenêtre de mesures au-dessus du budget fait descendre d'un palier, avec un message. */
  measure(ms: number) {
    this.avg = this.n ? this.avg * 0.9 + ms * 0.1 : ms
    if (++this.n < WINDOW) return
    this.n = 0
    debug.log('info', 'incrustation', `${this.avg.toFixed(1)} ms par rendu, palier ${this.level}`)
    if (this.avg <= BUDGET_MS || this.level >= LEVELS.length - 1) return
    this.level++
    debug.log('warn', 'incrustation', `réduite au palier ${this.level} (${this.avg.toFixed(1)} ms > ${BUDGET_MS} ms)`)
    this.onSlow?.("Incrustation de la caméra réduite : l'ordinateur ne suit pas.")
  }

  private render(w: number, h: number) {
    const v = this.video
    const o = prefs.camOverlay
    const matte = o.shape === 'free' ? this.matte?.() : undefined
    const r = overlayRect(o, w, h, v.videoWidth / v.videoHeight)
    const k = LEVELS[this.level].scale
    const lw = Math.max(2, Math.round(r.w * k))
    const lh = Math.max(2, Math.round(r.h * k))
    const layer = (this.layer ??= new OffscreenCanvas(lw, lh))
    if (layer.width !== lw || layer.height !== lh) { layer.width = lw; layer.height = lh }
    const c = layer.getContext('2d') as Ctx
    c.clearRect(0, 0, lw, lh)
    c.save()
    if (o.mirror) { c.translate(lw, 0); c.scale(-1, 1) }
    if (matte) {
      c.drawImage(v, 0, 0, lw, lh)
      c.globalCompositeOperation = 'destination-in'
      c.drawImage(matte, 0, 0, lw, lh)
    } else {
      c.beginPath()
      c.roundRect(0, 0, lw, lh, o.shape === 'circle' ? lw / 2 : Math.min(lw, lh) * 0.12)
      c.clip()
      const s = Math.min(v.videoWidth, v.videoHeight * (lw / lh)) // recadrage centré : le cercle prend le carré central
      const sh = s * (lh / lw)
      c.drawImage(v, (v.videoWidth - s) / 2, (v.videoHeight - sh) / 2, s, sh, 0, 0, lw, lh)
    }
    c.restore()
    if (o.border && !matte) {
      c.strokeStyle = '#fff'
      c.lineWidth = Math.max(2, lw * 0.015)
      c.beginPath()
      c.roundRect(0, 0, lw, lh, o.shape === 'circle' ? lw / 2 : Math.min(lw, lh) * 0.12)
      c.stroke()
    }
  }

  /** Pose la caméra sur `ctx` (image w × h). Le calque n'est refait qu'au rythme du palier courant. */
  draw(ctx: Ctx, w: number, h: number) {
    const v = this.video
    if (!v.videoWidth || !v.videoHeight) return
    const now = performance.now()
    if (now - this.last >= LEVELS[this.level].every) {
      this.last = now
      this.render(w, h)
      this.measure(performance.now() - now)
    }
    if (!this.layer) return
    const o = prefs.camOverlay
    const r = overlayRect(o, w, h, v.videoWidth / v.videoHeight)
    ctx.save()
    if (o.shadow) { ctx.shadowColor = 'rgba(0,0,0,.5)'; ctx.shadowBlur = Math.max(4, r.w * 0.04); ctx.shadowOffsetY = Math.max(2, r.w * 0.01) }
    ctx.drawImage(this.layer, r.x, r.y, r.w, r.h)
    ctx.restore()
  }

  /** Libère le calque et la lecture de la caméra. */
  destroy() {
    if (this.layer) { this.layer.width = 0; this.layer.height = 0 }
    this.layer = undefined
    this.video.pause()
    this.video.srcObject = null
  }
}

/** Source de la caméra : avec « Fond supprimé », l'image brute (la personne est découpée avec le masque d'EST-151), sinon la caméra publiée avec son effet. */
export async function openCamera(cam: LocalVideoTrack, onSlow?: (msg: string) => void) {
  const matte = cameraMatte(cam)
  const raw = matte
    ? await navigator.mediaDevices.getUserMedia({ video: { deviceId: prefs.cameraId ? { exact: prefs.cameraId } : undefined, width: { ideal: 1280 }, height: { ideal: 720 } } })
    : undefined
  const video = document.createElement('video')
  video.muted = true
  video.playsInline = true
  video.srcObject = new MediaStream([raw ? raw.getVideoTracks()[0] : cam.mediaStreamTrack])
  try {
    await video.play()
  } catch (e) {
    raw?.getTracks().forEach((t) => t.stop())
    throw e
  }
  const comp = new Composer(video, matte && (() => matte), onSlow)
  return { comp, close: () => { comp.destroy(); raw?.getTracks().forEach((t) => t.stop()) } }
}

/**
 * Compose la caméra sur chaque image du partage (insertable streams : pas de limitation d'une fenêtre en arrière-plan).
 * Une capture d'écran n'émet une image que quand le contenu change : sans relance, la caméra figerait sur un écran fixe.
 * Quand le partage reste muet, la dernière image est donc recomposée au rythme de la caméra.
 */
class OverlayTransformer implements VideoTrackTransformer<Record<string, unknown>> {
  transformer?: TransformStream
  private canvas?: OffscreenCanvas | HTMLCanvasElement
  private ctx?: Ctx
  private base = new OffscreenCanvas(1, 1)
  private bctx = this.base.getContext('2d')!
  private ctrl?: TransformStreamDefaultController<VideoFrame>
  private ts = 0
  private at = 0
  private timer?: ReturnType<typeof setInterval>

  constructor(private comp: Composer) {}

  async init({ outputCanvas }: VideoTransformerInitOptions) {
    this.setCanvas(outputCanvas)
    this.transformer = new TransformStream({ transform: (f, c) => this.transform(f, c) })
    this.timer = setInterval(() => {
      const idle = performance.now() - this.at
      if (this.ctrl && idle > 45) try { this.emit(this.ts + Math.round(idle * 1000)) } catch { /* flux fermé */ }
    }, 33)
  }
  async restart({ outputCanvas }: VideoTransformerInitOptions) { this.setCanvas(outputCanvas) }
  async destroy() {
    clearInterval(this.timer)
    this.ctrl = undefined
    this.base.width = 0
    this.base.height = 0
  }
  async update() {}

  private setCanvas(c: OffscreenCanvas | HTMLCanvasElement) {
    this.canvas = c
    this.ctx = c.getContext('2d') as Ctx
  }

  private emit(ts: number) {
    const { canvas, ctx, base } = this
    if (!canvas || !ctx || !this.ctrl || !base.width) return
    if (canvas.width !== base.width || canvas.height !== base.height) { canvas.width = base.width; canvas.height = base.height }
    ctx.drawImage(base, 0, 0)
    this.comp.draw(ctx, base.width, base.height)
    this.ts = Math.max(ts, this.ts + 1)
    this.ctrl.enqueue(new VideoFrame(canvas as OffscreenCanvas, { timestamp: this.ts }))
  }

  transform(frame: VideoFrame, controller: TransformStreamDefaultController<VideoFrame>) {
    try {
      const w = frame.displayWidth
      const h = frame.displayHeight
      if (!w || !h) return controller.enqueue(frame.clone())
      this.ctrl = controller
      if (this.base.width !== w || this.base.height !== h) { this.base.width = w; this.base.height = h }
      this.bctx.drawImage(frame, 0, 0, w, h)
      this.at = performance.now()
      this.emit(Math.max(frame.timestamp, this.ts + 1))
    } finally {
      frame.close()
    }
  }
}

/** Démarre l'incrustation sur le partage publié ; renvoie la fonction qui l'arrête et libère tout. */
export async function startOverlay(screen: LocalVideoTrack, cam: LocalVideoTrack, onSlow?: (msg: string) => void): Promise<() => Promise<void>> {
  const { ProcessorWrapper } = await import('@livekit/track-processors')
  const { comp, close } = await openCamera(cam, onSlow)
  try {
    await screen.setProcessor(new ProcessorWrapper(new OverlayTransformer(comp), 'estompe-incrustation'))
  } catch (e) {
    close()
    throw e
  }
  debug.log('info', 'incrustation', 'activée')
  return async () => {
    await screen.stopProcessor().catch(() => {})
    close()
    debug.log('info', 'incrustation', 'désactivée')
  }
}
