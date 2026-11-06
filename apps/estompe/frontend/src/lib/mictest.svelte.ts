import type { AudioProcessorOptions, Track } from 'livekit-client'
import { prefs, type NoiseMode } from './prefs.svelte'
import { audioConstraints } from '../blocks/vocal/voice.svelte'
import { MicProcessor } from './micprocessor'

export const MIC_TEST_MAX_S = 10
export const NOISE_MODES: NoiseMode[] = ['off', 'standard', 'strong']

/** Même règle qu'en vocal : push-to-talk = touche enfoncée, détection de voix = voix au-dessus du seuil, sinon toujours ouvert. */
export function gateOpen(ptt: boolean, vad: boolean, talking: boolean, active: boolean) {
  return ptt ? talking : vad ? active : true
}

interface Chain { stream: MediaStream; proc: MicProcessor; out: MediaStream; rec?: MediaRecorder; chunks: Blob[] }

/** Test du micro des paramètres : la chaîne de traitement du vocal (micro choisi, réduction de bruit, volume, porte), jamais envoyée. */
class MicTest {
  live = $state(false)
  recording = $state(false)
  playing = $state<NoiseMode | null>(null)
  clip = $state(false)
  level = $state(-100)
  active = $state(false)
  talking = $state(false)
  elapsed = $state(0)
  error = $state('')
  private ctx?: AudioContext
  private chains = new Map<NoiseMode, Chain>()
  private urls = new Map<NoiseMode, string>()
  private el?: HTMLAudioElement
  private poll: ReturnType<typeof setInterval> | null = null
  private lastAbove = 0
  private off: (() => void) | null = null

  private async build(mode: NoiseMode, record: boolean) {
    const on = mode !== 'off'
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { ...audioConstraints(), echoCancellation: on, noiseSuppression: on, autoGainControl: on } })
    const track = stream.getAudioTracks()[0]
    const proc = new MicProcessor(mode === 'strong', prefs.micGain / 100)
    await proc.init({ kind: 'audio' as Track.Kind.Audio, track, audioContext: this.ctx! } as AudioProcessorOptions)
    const out = new MediaStream([proc.processedTrack!])
    const chain: Chain = { stream, proc, out, chunks: [] }
    if (record) {
      const rec = new MediaRecorder(out)
      rec.ondataavailable = (e) => e.data.size && chain.chunks.push(e.data)
      chain.rec = rec
    }
    this.chains.set(mode, chain)
  }

  private async open(modes: NoiseMode[], record: boolean) {
    this.error = ''
    this.ctx = new AudioContext()
    try {
      for (const m of modes) await this.build(m, record)
    } catch (e) {
      this.close()
      this.error = e instanceof DOMException && e.name === 'NotAllowedError' ? 'Accès au micro refusé.' : 'Micro inutilisable.'
      return false
    }
    this.talking = false
    this.lastAbove = 0
    this.watch()
    return true
  }

  private close() {
    if (this.poll) clearInterval(this.poll)
    this.poll = null
    this.off?.()
    this.off = null
    for (const c of this.chains.values()) {
      if (c.rec && c.rec.state !== 'inactive') c.rec.stop()
      void c.proc.destroy()
      c.stream.getTracks().forEach((t) => t.stop())
    }
    this.chains.clear()
    void this.ctx?.close()
    this.ctx = undefined
    this.live = this.recording = false
    this.level = -100
    this.active = false
    this.sound(null)
  }

  /** Lit le niveau, applique la porte et compte la durée enregistrée ; écoute la touche de push-to-talk. */
  private watch() {
    const key = (down: boolean) => (e: KeyboardEvent) => {
      if (e.code === prefs.pttKey && prefs.ptt) this.setTalking(down)
    }
    const kd = key(true)
    const ku = key(false)
    const blur = () => this.setTalking(false)
    addEventListener('keydown', kd)
    addEventListener('keyup', ku)
    addEventListener('blur', blur)
    this.off = () => {
      removeEventListener('keydown', kd)
      removeEventListener('keyup', ku)
      removeEventListener('blur', blur)
    }
    this.poll = setInterval(() => this.tick(66), 66)
  }

  /** @internal exposé pour les tests */
  tick(ms: number) {
    const db = this.chains.get(prefs.noise)?.proc.level() ?? -100
    this.level = Math.round(db)
    const now = Date.now()
    if (db > prefs.vadThreshold) this.lastAbove = now
    this.active = now - this.lastAbove < 400
    this.applyGate()
    if (this.recording && (!prefs.ptt || this.talking)) {
      this.elapsed += ms / 1000
      if (this.elapsed >= MIC_TEST_MAX_S) this.stopRecording()
    }
  }

  private applyGate() {
    const open = gateOpen(prefs.ptt, prefs.vad, this.talking, this.active)
    for (const c of this.chains.values()) c.proc.setGate(open)
  }

  setTalking(on: boolean) {
    if (this.talking === on) return
    this.talking = on
    this.applyGate()
    if (!this.recording) return
    for (const c of this.chains.values()) {
      if (on && c.rec?.state === 'paused') c.rec.resume()
      else if (!on && prefs.ptt && c.rec?.state === 'recording') c.rec.pause()
    }
  }

  private sound(src: MediaStream | string | null, mode: NoiseMode | null = null) {
    if (this.el) {
      this.el.pause()
      this.el.srcObject = null
      this.el.removeAttribute('src')
    }
    this.playing = mode
    if (!src) return
    this.el ??= new Audio()
    const el = this.el
    void (el as HTMLAudioElement & { setSinkId?(id: string): Promise<void> }).setSinkId?.(prefs.speakerId)
    if (typeof src === 'string') el.src = src
    else el.srcObject = src
    el.onended = () => { if (this.playing) this.playing = null }
    void el.play()
  }

  async startLive() {
    this.stop()
    if (!(await this.open([prefs.noise], false))) return
    this.live = true
    this.sound(this.chains.get(prefs.noise)!.out)
  }

  /** Enregistre le même son avec les trois niveaux de réduction de bruit, pour les comparer à la réécoute. */
  async record() {
    this.stop()
    this.clearClips()
    if (!(await this.open(NOISE_MODES, true))) return
    this.elapsed = 0
    this.recording = true
    for (const c of this.chains.values()) {
      c.rec!.start()
      if (prefs.ptt) c.rec!.pause()
    }
  }

  stopRecording() {
    if (!this.recording) return
    const chains = [...this.chains.entries()]
    this.recording = false
    let left = chains.length
    for (const [mode, c] of chains) {
      c.rec!.onstop = () => {
        this.urls.set(mode, URL.createObjectURL(new Blob(c.chunks, { type: c.rec!.mimeType })))
        if (--left === 0) this.clip = true
      }
      if (c.rec!.state !== 'inactive') c.rec!.stop()
    }
    this.close()
  }

  play(mode: NoiseMode) {
    const url = this.urls.get(mode)
    if (!url) return
    this.stop()
    this.sound(url, mode)
  }

  /** Les réglages changés pendant l'écoute en direct s'appliquent tout de suite ; le volume aussi pendant un enregistrement. */
  refresh() {
    for (const c of this.chains.values()) c.proc.setGain(prefs.micGain / 100)
    if (this.live) void this.startLive()
  }

  /** Arrête l'écoute, l'enregistrement et la réécoute ; l'enregistrement fini est gardé jusqu'à `reset`. */
  stop() {
    if (this.recording) return this.stopRecording()
    this.close()
  }

  private clearClips() {
    this.urls.forEach((u) => URL.revokeObjectURL(u))
    this.urls.clear()
    this.clip = false
  }

  reset() {
    this.stop()
    this.clearClips()
  }
}

export const micTest = new MicTest()
