import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { audioConstraints, micError, voice, voiceStates } from './voice.svelte'
import { gateOpen, micTest } from '../../lib/mictest.svelte'
import { prefs } from '../../lib/prefs.svelte'
import { applyCameraFx } from '../../lib/camerafx'
import { Composer, overlayRect } from '../../lib/camoverlay'
import { platform as web } from '@platform'
import { setPlatform } from '../../lib/platform'

const switchTo = vi.fn()
const bgProcessor = vi.fn((t: { opts: Record<string, unknown> }) => ({ t, updateTransformerOptions: switchTo }))
vi.mock('@livekit/track-processors', () => ({ ProcessorWrapper: class { constructor(t: never) { return bgProcessor(t) as never } } }))
vi.mock('../../lib/person', () => ({ PersonTransformer: class { constructor(public opts: Record<string, unknown>) {} } }))

const procs: { setGain: ReturnType<typeof vi.fn>; setGate: ReturnType<typeof vi.fn>; destroy: ReturnType<typeof vi.fn>; rnnoise: boolean }[] = []
vi.mock('../../lib/micprocessor', () => ({
  MicProcessor: class {
    processedTrack = {}
    setGain = vi.fn()
    setGate = vi.fn()
    destroy = vi.fn()
    level = () => -30
    constructor(public rnnoise: boolean) { procs.push(this as never) }
    async init() {}
  },
}))

describe('micError', () => {
  it('indique la cause et l\'écoute seule', () => {
    expect(micError(new DOMException('', 'NotAllowedError'))).toMatch(/refusé.*écoute seule/)
    expect(micError(new DOMException('', 'NotFoundError'))).toMatch(/Aucun micro.*écoute seule/)
    expect(micError(new Error('x'))).toMatch(/inutilisable.*écoute seule/)
  })
})

describe('audioConstraints', () => {
  it('capture le micro en mono', () => {
    expect(audioConstraints().channelCount).toBe(1)
  })
})

describe('gateOpen', () => {
  it('suit push-to-talk, détection de voix ou reste ouverte', () => {
    expect(gateOpen(true, true, false, true)).toBe(false)
    expect(gateOpen(true, false, true, false)).toBe(true)
    expect(gateOpen(false, true, false, false)).toBe(false)
    expect(gateOpen(false, true, false, true)).toBe(true)
    expect(gateOpen(false, false, false, false)).toBe(true)
  })
})

describe('micTest', () => {
  const gum = vi.fn()
  const recs: { state: string; pause: () => void; resume: () => void }[] = []
  beforeEach(() => {
    procs.length = recs.length = 0
    gum.mockReset().mockImplementation(async () => ({ getAudioTracks: () => [{}], getTracks: () => [{ stop() {} }] }))
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: gum } })
    vi.stubGlobal('AudioContext', class { close = vi.fn() })
    vi.stubGlobal('MediaStream', class {})
    vi.stubGlobal('Audio', class { play = vi.fn(); pause = vi.fn(); removeAttribute = vi.fn() })
    vi.stubGlobal('MediaRecorder', class {
      state = 'inactive'; mimeType = 'audio/webm'; ondataavailable?: unknown; onstop?: () => void
      constructor() { recs.push(this as never) }
      start() { this.state = 'recording' }
      pause() { this.state = 'paused' }
      resume() { this.state = 'recording' }
      stop() { this.state = 'inactive'; this.onstop?.() }
    })
    URL.createObjectURL = vi.fn(() => 'blob:x')
    URL.revokeObjectURL = vi.fn()
    Object.assign(prefs, { noise: 'strong', ptt: false, vad: false })
  })
  afterEach(() => { micTest.reset(); vi.unstubAllGlobals() })

  it('démarre l\'écoute avec le traitement choisi et l\'arrête proprement', async () => {
    await micTest.startLive()
    expect(micTest.live).toBe(true)
    expect(procs).toHaveLength(1)
    expect(procs[0].rnnoise).toBe(true)
    micTest.stop()
    expect(micTest.live).toBe(false)
    expect(procs[0].destroy).toHaveBeenCalled()
  })

  it('applique le volume et le changement de bruit tout de suite', async () => {
    await micTest.startLive()
    prefs.micGain = 200
    prefs.noise = 'off'
    micTest.refresh()
    expect(procs[0].setGain).toHaveBeenCalledWith(2)
    await vi.waitFor(() => expect(procs).toHaveLength(2))
    expect(procs[1].rnnoise).toBe(false)
  })

  it('enregistre les trois niveaux de bruit, 10 s au plus, et les garde', async () => {
    await micTest.record()
    expect(recs).toHaveLength(3)
    micTest.tick(11000)
    expect(micTest.recording).toBe(false)
    expect(micTest.clip).toBe(true)
  })

  it('en push-to-talk, n\'enregistre que touche enfoncée', async () => {
    prefs.ptt = true
    await micTest.record()
    expect(recs.every((r) => r.state === 'paused')).toBe(true)
    micTest.tick(5000)
    expect(micTest.elapsed).toBe(0)
    micTest.setTalking(true)
    expect(recs.every((r) => r.state === 'recording')).toBe(true)
    micTest.tick(2000)
    expect(micTest.elapsed).toBe(2)
    micTest.setTalking(false)
    expect(recs.every((r) => r.state === 'paused')).toBe(true)
  })
})

describe('applyCameraFx (EST-151)', () => {
  const track = () => ({ setProcessor: vi.fn(), stopProcessor: vi.fn().mockResolvedValue(undefined) })
  beforeEach(() => {
    bgProcessor.mockClear()
    switchTo.mockClear()
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ createLinearGradient: () => ({ addColorStop() {} }), fillRect() {} } as never)
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/jpeg;base64,')
  })
  afterEach(() => { prefs.cameraFx = 'none'; vi.restoreAllMocks() })

  it('« Aucun » ne charge aucun modèle et ne touche pas à la caméra', async () => {
    const t = track()
    prefs.cameraFx = 'none'
    expect(await applyCameraFx(t as never)).toBe(true)
    expect(bgProcessor).not.toHaveBeenCalled()
    expect(t.setProcessor).not.toHaveBeenCalled()
  })

  it('charge le processeur au premier choix, le met à jour à la volée, puis le libère', async () => {
    const t = track()
    prefs.cameraFx = 'blur'
    await applyCameraFx(t as never)
    expect(bgProcessor).toHaveBeenCalledWith(expect.objectContaining({ opts: expect.objectContaining({ blurRadius: prefs.cameraBlur }) }))
    expect(t.setProcessor).toHaveBeenCalledTimes(1)

    prefs.cameraFx = 'cutout'
    await applyCameraFx(t as never)
    expect(switchTo).toHaveBeenCalledWith(expect.objectContaining({ imagePath: expect.any(String) }))
    expect(bgProcessor).toHaveBeenCalledTimes(1)

    prefs.cameraFx = 'none'
    await applyCameraFx(t as never)
    expect(t.stopProcessor).toHaveBeenCalledTimes(1)
  })

  it('signale un effet impossible et libère la caméra', async () => {
    const t = track()
    t.setProcessor.mockRejectedValue(new Error('webgl'))
    prefs.cameraFx = 'blur'
    expect(await applyCameraFx(t as never)).toBe(false)
    expect(t.stopProcessor).toHaveBeenCalled()
  })
})

describe('incrustation de la caméra (EST-150)', () => {
  const o = { x: 1, y: 1, size: 0.25, shape: 'rect' as const, margin: 20 }

  it('place le rectangle dans le coin choisi avec la marge', () => {
    expect(overlayRect(o, 1920, 1080, 16 / 9)).toEqual({ x: 1920 - 480 - 20, y: 1080 - 270 - 20, w: 480, h: 270 })
    expect(overlayRect({ ...o, x: 0, y: 0 }, 1920, 1080, 16 / 9)).toMatchObject({ x: 20, y: 20 })
  })

  it('cercle carré, et marge réduite pour ne jamais sortir de l\'image', () => {
    expect(overlayRect({ ...o, shape: 'circle' }, 1920, 1080, 16 / 9)).toMatchObject({ w: 480, h: 480 })
    const r = overlayRect({ ...o, size: 0.5, margin: 5000 }, 1000, 1000, 1)
    expect(r.x).toBeGreaterThanOrEqual(0)
    expect(r.x + r.w).toBeLessThanOrEqual(1000)
  })

  it('descend d\'un palier avec un message quand le rendu dépasse le budget, puis ne descend plus sous le dernier', () => {
    const onSlow = vi.fn()
    const c = new Composer(document.createElement('video'), undefined, onSlow)
    for (let i = 0; i < 30; i++) c.measure(2)
    expect(c.level).toBe(0)
    for (let i = 0; i < 30 * 5; i++) c.measure(20)
    expect(c.level).toBe(2)
    expect(onSlow).toHaveBeenCalledTimes(2)
  })

  it('libère la lecture de la caméra', () => {
    const v = document.createElement('video')
    v.pause = vi.fn()
    new Composer(v).destroy()
    expect(v.pause).toHaveBeenCalled()
    expect(v.srcObject).toBeNull()
  })
})

describe('voiceStates (EST-162)', () => {
  const ps: { identity: string; attributes: Record<string, string> }[] = [{ identity: 'a', attributes: { muted: '1' } }, { identity: 'b', attributes: { muted: '1', deaf: '1' } }, { identity: 'c', attributes: {} }]
  it('affiche le micro coupé et la sourdine', () => {
    expect(voiceStates(ps, {})).toEqual({ a: { muted: true, deaf: false }, b: { muted: true, deaf: true }, c: { muted: false, deaf: false } })
  })
  it("n'affiche jamais le micro coupé de qui parle", () => {
    expect(voiceStates(ps, { a: true }).a.muted).toBe(false)
  })
})

describe('watching (EST-163)', () => {
  it('ne republie pas les attributs quand rien ne change', () => {
    const setAttributes = vi.fn()
    ;(voice as unknown as { room: unknown }).room = { localParticipant: { setAttributes } }
    const off = voice.view('s1')
    off()
    voice.view('s1')
    voice.view('s1')
    expect(setAttributes.mock.calls.map((c) => c[0].watching)).toEqual(['s1', '', 's1'])
    ;(voice as unknown as { room: unknown }).room = undefined
  })
})

describe('vocal en arrière-plan', () => {
  it('prévient la plateforme quand le vocal se termine', async () => {
    const update = vi.fn()
    setPlatform({ ...web, voiceCall: { update } })
    await voice.leave()
    setPlatform(web)
    expect(update).toHaveBeenCalledWith({ channelId: null, muted: false, deafened: false })
  })
})
