// Arrière-plan de la caméra : détourage de la personne avec lissage dans le temps (EST-151).
// Le masque de confiance (0 à 1) est moyenné d'une image à l'autre puis adouci, ce qui supprime le scintillement
// des bords que donne un masque de catégorie (oui/non) recalculé à chaque image.
import type { ImageSegmenter, MPMask } from '@mediapipe/tasks-vision'
import type { VideoTrackTransformer, VideoTransformerInitOptions } from '@livekit/track-processors'

export interface PersonOptions extends Record<string, unknown> {
  /** Flou de l'arrière-plan, en pixels d'une image de 720 px de haut ; sans effet si `imagePath` est défini. */
  blurRadius?: number
  /** Image de fond (URL ou data:) ; sinon l'arrière-plan est flouté. */
  imagePath?: string
  assetPaths: { tasksVisionFileSet: string; modelAssetPath: string }
  onFrameProcessed?: (stats: { processingTimeMs: number }) => void
}

type Ctx = OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D
const smooth = (lo: number, hi: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - lo) / (hi - lo)))
  return t * t * (3 - 2 * t)
}

export class PersonTransformer implements VideoTrackTransformer<PersonOptions> {
  transformer?: TransformStream
  /** Dernier masque lissé de la personne (0 à 1), réutilisable par d'autres effets (incrustation, EST-150). */
  mask?: { data: Float32Array; width: number; height: number }
  private canvas?: OffscreenCanvas | HTMLCanvasElement
  private ctx?: Ctx
  private person = new OffscreenCanvas(1, 1)
  private pctx = this.person.getContext('2d')!
  // Le modèle travaille en 256 px : on lui donne cette taille, le masque (et son lissage) reste léger.
  private small = new OffscreenCanvas(256, 144)
  private sctx = this.small.getContext('2d')!
  /** Masque de la personne en alpha (à la taille du masque agrandi), réutilisé par l'incrustation du partage d'écran (EST-150). */
  readonly alpha = new OffscreenCanvas(1, 1)
  private actx = this.alpha.getContext('2d')!
  private seg?: ImageSegmenter
  private image?: HTMLImageElement
  private idx = 1
  private ts = 0

  constructor(private opts: PersonOptions) {}

  async init({ outputCanvas }: VideoTransformerInitOptions) {
    this.setCanvas(outputCanvas)
    this.transformer = new TransformStream({ transform: (f, c) => this.transform(f, c) })
    await this.update({})
    if (this.seg) return
    const vision = await import('@mediapipe/tasks-vision')
    const files = await vision.FilesetResolver.forVisionTasks(this.opts.assetPaths.tasksVisionFileSet)
    const make = (delegate: 'GPU' | 'CPU') =>
      vision.ImageSegmenter.createFromOptions(files, {
        baseOptions: { modelAssetPath: this.opts.assetPaths.modelAssetPath, delegate },
        runningMode: 'VIDEO',
        outputCategoryMask: false,
        outputConfidenceMasks: true,
      })
    this.seg = await make('GPU').catch(() => make('CPU'))
    // Le masque à lire est celui de la personne : l'indice de l'étiquette si le modèle en donne, sinon le second (fond, personne).
    const i = this.seg.getLabels().findIndex((l) => /person|selfie|foreground/i.test(l))
    this.idx = i >= 0 ? i : 1
  }

  async restart({ outputCanvas }: VideoTransformerInitOptions) {
    this.setCanvas(outputCanvas)
  }

  async destroy() {
    this.seg?.close()
    this.seg = undefined
    this.mask = undefined
  }

  async update(o: Partial<PersonOptions>) {
    const prev = this.opts.imagePath
    this.opts = { ...this.opts, ...o }
    const path = this.opts.imagePath
    if (!path) this.image = undefined
    else if (path !== prev || !this.image) {
      const img = new Image()
      await new Promise<void>((ok, ko) => { img.onload = () => ok(); img.onerror = ko; img.src = path })
      this.image = img
    }
  }

  private setCanvas(c: OffscreenCanvas | HTMLCanvasElement) {
    this.canvas = c
    this.ctx = c.getContext('2d') as Ctx
  }

  async transform(frame: VideoFrame, controller: TransformStreamDefaultController<VideoFrame>) {
    const t0 = performance.now()
    try {
      const w = frame.displayWidth
      const h = frame.displayHeight
      const { canvas, ctx, seg } = this
      if (!canvas || !ctx || !seg || !w || !h) return controller.enqueue(frame.clone())
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h }
      if (this.person.width !== w || this.person.height !== h) { this.person.width = w; this.person.height = h }

      this.ts = Math.max(this.ts + 1, performance.now())
      const sh = Math.max(1, Math.round((256 * h) / w))
      if (this.small.height !== sh) this.small.height = sh
      this.sctx.drawImage(frame, 0, 0, 256, sh)
      this.updateMask(seg.segmentForVideo(this.small, this.ts))

      // Arrière-plan : image recadrée sur le cadre, ou image de la caméra floutée.
      ctx.globalCompositeOperation = 'source-over'
      if (this.image) {
        const iw = this.image.naturalWidth
        const ih = this.image.naturalHeight
        const k = Math.max(w / iw, h / ih)
        ctx.drawImage(this.image, (iw - w / k) / 2, (ih - h / k) / 2, w / k, h / k, 0, 0, w, h)
      } else {
        const r = Math.max(1, (this.opts.blurRadius ?? 10) * (h / 720))
        ctx.filter = `blur(${r}px)`
        ctx.drawImage(frame, -2 * r, -2 * r, w + 4 * r, h + 4 * r) // débord : le flou ne laisse pas de liseré transparent
        ctx.filter = 'none'
      }

      // Personne : l'image de la caméra, ne gardant que ce que le masque (adouci en l'agrandissant) laisse passer.
      const p = this.pctx
      p.globalCompositeOperation = 'source-over'
      p.clearRect(0, 0, w, h)
      p.drawImage(frame, 0, 0, w, h)
      p.globalCompositeOperation = 'destination-in'
      p.imageSmoothingQuality = 'high'
      p.drawImage(this.alpha, 0, 0, w, h)
      ctx.drawImage(this.person, 0, 0, w, h)

      controller.enqueue(new VideoFrame(canvas as OffscreenCanvas, { timestamp: frame.timestamp }))
      this.opts.onFrameProcessed?.({ processingTimeMs: performance.now() - t0 })
    } finally {
      frame.close()
    }
  }

  /**
   * Moyenne glissante du masque (les petits écarts sont amortis, les vrais mouvements suivis presque tout de suite),
   * agrandie en bilinéaire AVANT le seuil : les contours sont lisses, sans cases ni damier là où la confiance hésite.
   */
  private updateMask(res: { confidenceMasks?: MPMask[]; close(): void }) {
    const m = res.confidenceMasks?.[Math.min(this.idx, (res.confidenceMasks?.length ?? 1) - 1)]
    if (!m) return res.close()
    const { width: mw, height: mh } = m
    const cur = m.getAsFloat32Array()
    if (!this.mask || this.mask.width !== mw || this.mask.height !== mh) this.mask = { data: new Float32Array(cur), width: mw, height: mh }
    const prev = this.mask.data
    for (let i = 0; i < cur.length; i++) {
      const d = cur[i] - prev[i]
      prev[i] += d * Math.min(1, 0.4 + 3 * Math.abs(d))
    }
    res.close()

    const W = 640
    const H = Math.round((W * mh) / mw)
    if (this.alpha.width !== W || this.alpha.height !== H) { this.alpha.width = W; this.alpha.height = H }
    const img = new ImageData(W, H)
    const out = img.data
    const xs = new Int32Array(W)
    const fxs = new Float32Array(W)
    for (let x = 0; x < W; x++) {
      const sx = Math.min(mw - 1, Math.max(0, ((x + 0.5) * mw) / W - 0.5))
      xs[x] = Math.min(mw - 2, Math.floor(sx))
      fxs[x] = sx - xs[x]
    }
    for (let y = 0; y < H; y++) {
      const sy = Math.min(mh - 1, Math.max(0, ((y + 0.5) * mh) / H - 0.5))
      const y0 = Math.min(mh - 2, Math.floor(sy))
      const fy = sy - y0
      const r0 = y0 * mw
      const r1 = r0 + mw
      for (let x = 0; x < W; x++) {
        const x0 = xs[x]
        const fx = fxs[x]
        const top = prev[r0 + x0] * (1 - fx) + prev[r0 + x0 + 1] * fx
        const bot = prev[r1 + x0] * (1 - fx) + prev[r1 + x0 + 1] * fx
        out[(y * W + x) * 4 + 3] = Math.round(255 * smooth(0.25, 0.55, top * (1 - fy) + bot * fy))
      }
    }
    this.actx.putImageData(img, 0, 0)
  }
}
