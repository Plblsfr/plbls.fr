import { prefs } from './prefs.svelte'
import { VOICE_MAX_MS, levelOf, recordingType, toWaveform } from './files'

/** Enregistrement terminé, prêt à écouter puis à envoyer. */
export interface Take {
  blob: Blob
  name: string
  duration: number
  waveform: number[]
}

/** Un enregistrement vocal : « idle » hors usage, « recording » en cours, « review » une fois arrêté (écoute, envoi). */
export class Recorder {
  state = $state<'idle' | 'recording' | 'review'>('idle')
  elapsed = $state(0)
  levels = $state<number[]>([])
  take = $state<Take | null>(null)

  private rec: MediaRecorder | null = null
  private stream: MediaStream | null = null
  private ctx: AudioContext | null = null
  private chunks: Blob[] = []
  private type = ''
  private ext = ''
  private t0 = 0
  private timer = 0

  /** Ouvre le micro choisi dans les Paramètres et commence l'enregistrement. Lève une erreur si le micro est refusé. */
  async start() {
    const fmt = recordingType()
    if (!fmt) throw new Error('Enregistrement vocal indisponible sur ce navigateur.')
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { deviceId: prefs.micId || undefined, channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
    })
    this.stream = stream
    this.ctx = new AudioContext()
    const analyser = this.ctx.createAnalyser()
    analyser.fftSize = 1024
    this.ctx.createMediaStreamSource(stream).connect(analyser)
    this.rec = new MediaRecorder(stream, { mimeType: fmt.mime, audioBitsPerSecond: 32000 })
    this.type = fmt.mime
    this.ext = fmt.ext
    this.chunks = []
    this.rec.ondataavailable = (e) => { if (e.data.size) this.chunks.push(e.data) }
    this.rec.start(1000)
    this.t0 = Date.now()
    this.levels = []
    this.elapsed = 0
    this.state = 'recording'
    const buf = new Float32Array(analyser.fftSize)
    this.timer = window.setInterval(() => {
      analyser.getFloatTimeDomainData(buf)
      this.levels.push(levelOf(buf))
      this.elapsed = Date.now() - this.t0
      if (this.elapsed >= VOICE_MAX_MS) void this.stop()
    }, 100)
  }

  /** Arrête l'enregistrement et passe à l'écoute : `take` est alors prêt. */
  async stop() {
    if (this.state !== 'recording' || !this.rec) return
    const rec = this.rec
    clearInterval(this.timer)
    await new Promise<void>((resolve) => {
      rec.onstop = () => resolve()
      if (rec.state === 'inactive') resolve()
      else rec.stop()
    })
    this.release()
    this.take = {
      blob: new Blob(this.chunks, { type: this.type }),
      name: `message-vocal.${this.ext}`,
      duration: Math.min(VOICE_MAX_MS, Math.max(1, Date.now() - this.t0)),
      waveform: toWaveform(this.levels),
    }
    this.state = 'review'
  }

  /** Abandonne l'enregistrement en cours ou l'écoute, sans rien envoyer. */
  cancel() {
    if (this.state === 'recording' && this.rec) {
      clearInterval(this.timer)
      this.rec.onstop = null
      this.rec.stop()
      this.release()
    }
    this.take = null
    this.levels = []
    this.elapsed = 0
    this.state = 'idle'
  }

  private release() {
    this.stream?.getTracks().forEach((t) => t.stop())
    void this.ctx?.close()
    this.stream = null
    this.ctx = null
    this.rec = null
  }
}
