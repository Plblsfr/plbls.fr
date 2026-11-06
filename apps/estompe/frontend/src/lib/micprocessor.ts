import type { AudioProcessorOptions, Track, TrackProcessor } from 'livekit-client'

/** Niveau efficace d'un bloc d'échantillons (de -1 à 1) en dBFS ; -100 pour le silence. */
export function rmsDb(samples: ArrayLike<number>): number {
  if (!samples.length) return -100
  let sum = 0
  for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i]
  const rms = Math.sqrt(sum / samples.length)
  return rms < 1e-5 ? -100 : Math.max(-100, 20 * Math.log10(rms))
}

/** Force un nœud en mono de bout en bout : un signal à un canal est dupliqué à gauche et à droite en sortie (EST-141). */
export function forceMono<T extends AudioNode>(n: T): T {
  n.channelCount = 1
  n.channelCountMode = 'explicit'
  n.channelInterpretation = 'speakers'
  return n
}

/** Traitement du micro : suppression de bruit RNNoise (WASM, local, atténue notamment les clics de clavier) puis gain réglable. Chargé à la demande. */
export class MicProcessor implements TrackProcessor<Track.Kind.Audio, AudioProcessorOptions> {
  name = 'estompe-mic'
  processedTrack?: MediaStreamTrack
  private nodes: AudioNode[] = []
  private rn?: { destroy(): void }
  private gain?: GainNode
  private gate?: GainNode
  private gateOpen = true
  private analyser?: AnalyserNode
  private buf?: Float32Array<ArrayBuffer>

  /** @param rnnoise active la suppression de bruit renforcée ; @param gain multiplicateur du volume (1 = inchangé, jusqu'à 3). */
  constructor(private rnnoise: boolean, private gainLevel = 1) {}

  setGain(v: number) {
    this.gainLevel = v
    if (this.gain) this.gain.gain.value = v
  }

  /**
   * Porte de la détection de voix : fermée, le micro envoie du silence (la piste reste publiée et mesurée,
   * ce qu'une coupure de la piste empêcherait).
   */
  setGate(open: boolean) {
    this.gateOpen = open
    if (this.gate && this.gate.context.state !== 'closed') this.gate.gain.setTargetAtTime(open ? 1 : 0, this.gate.context.currentTime, 0.01)
  }

  /** Niveau actuel du micro (après la suppression de bruit, avant le gain), en dBFS. */
  level(): number {
    if (!this.analyser || !this.buf) return -100
    this.analyser.getFloatTimeDomainData(this.buf)
    return rmsDb(this.buf)
  }

  async init(opts: AudioProcessorOptions) {
    const ctx = opts.audioContext
    const source = ctx.createMediaStreamSource(new MediaStream([opts.track]))
    const dest = forceMono(ctx.createMediaStreamDestination())
    const gain = forceMono(ctx.createGain())
    gain.gain.value = this.gainLevel
    this.gain = gain
    this.nodes = [source, gain, dest]
    let last: AudioNode = source
    if (this.rnnoise) {
      const [lib, worklet, wasm, simd] = await Promise.all([
        import('@sapphi-red/web-noise-suppressor'),
        import('@sapphi-red/web-noise-suppressor/rnnoiseWorklet.js?url'),
        import('@sapphi-red/web-noise-suppressor/rnnoise.wasm?url'),
        import('@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm?url'),
      ])
      await ctx.audioWorklet.addModule(worklet.default)
      const wasmBinary = await lib.loadRnnoise({ url: wasm.default, simdUrl: simd.default })
      const node = forceMono(new lib.RnnoiseWorkletNode(ctx, { maxChannels: 1, wasmBinary }))
      last.connect(node)
      last = node
      this.nodes.push(node)
      this.rn = node
    }
    const analyser = ctx.createAnalyser()
    analyser.fftSize = 1024
    last.connect(analyser)
    this.analyser = analyser
    this.buf = new Float32Array(analyser.fftSize)
    this.nodes.push(analyser)
    const gate = forceMono(ctx.createGain())
    gate.gain.value = this.gateOpen ? 1 : 0
    this.gate = gate
    this.nodes.push(gate)
    last.connect(gain).connect(gate).connect(dest)
    this.processedTrack = dest.stream.getAudioTracks()[0]
  }

  async restart(opts: AudioProcessorOptions) {
    await this.destroy()
    await this.init(opts)
  }

  async destroy() {
    this.rn?.destroy()
    this.nodes.forEach((n) => n.disconnect())
    this.nodes = []
    this.rn = undefined
    this.gain = undefined
    this.gate = undefined
    this.analyser = undefined
    this.buf = undefined
    this.processedTrack = undefined
  }
}
