import { rmsDb } from '../../lib/micprocessor'

const THRESHOLD_DB = -48 // en dessous : silence ou souffle
const HOLD_MS = 250 // évite que la bordure clignote entre deux mots

let ctx: AudioContext | null = null

/** Surveille le niveau réel d'une piste audio (sans la rejouer) et signale quand on parle. Retourne la fonction d'arrêt. */
export function watchSpeaking(track: MediaStreamTrack, onChange: (speaking: boolean) => void): () => void {
  try {
    ctx ??= new AudioContext()
    const node = ctx.createAnalyser()
    node.fftSize = 1024
    const src = ctx.createMediaStreamSource(new MediaStream([track]))
    src.connect(node) // pas relié à la sortie : aucun son en double
    const buf = new Float32Array(node.fftSize)
    let on = false
    let last = 0
    const id = setInterval(() => {
      node.getFloatTimeDomainData(buf)
      const now = performance.now()
      if (track.readyState === 'live' && track.enabled && rmsDb(buf) > THRESHOLD_DB) last = now
      const next = now - last < HOLD_MS
      if (next !== on) onChange((on = next))
    }, 50)
    return () => {
      clearInterval(id)
      src.disconnect()
      if (on) onChange(false)
    }
  } catch {
    return () => {} // audio indisponible : pas de bordure
  }
}
