import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import { castRefusal, type FilmPlayback } from '../../sections/entracte/films'
import { providers, type PState } from './providers'

const hlsjs = vi.hoisted(() => ({ loadSource: vi.fn(), attachMedia: vi.fn() }))
vi.mock('hls.js', () => ({
  default: class {
    static isSupported = () => true
    static Events = { ERROR: 'hlsError', MANIFEST_PARSED: 'hlsManifestParsed' }
    on() {}
    loadSource(u: string) { hlsjs.loadSource(u) }
    attachMedia(v: HTMLMediaElement) { hlsjs.attachMedia(v) }
    destroy() {}
  },
}))

const flux = (n: number, mode: 'direct' | 'hls' = 'direct'): FilmPlayback => ({ stream_id: `s${n}`, mode, url: `/stream/j${n}/${mode === 'direct' ? 'direct' : 'hls.m3u8'}`, duration: 3000, audio: [], subtitles: [] })

function largeur(video: HTMLVideoElement, w: number) {
  Object.defineProperty(video, 'videoWidth', { configurable: true, get: () => w })
}

describe('lecteur de films : repli sur le flux transcodé', () => {
  let play: ReturnType<typeof vi.fn>
  beforeEach(() => {
    vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    play = vi.fn().mockResolvedValue(undefined)
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockImplementation(play as never)
    vi.spyOn(filmsApi, 'close').mockResolvedValue(undefined as never)
    vi.spyOn(filmsApi, 'progress').mockResolvedValue(undefined as never)
  })
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks() })

  it('rouvre en HLS une lecture directe dont la vidéo n’est pas décodée (image absente)', async () => {
    let n = 0
    const playback = vi.spyOn(filmsApi, 'playback').mockImplementation(async () => flux(++n, n === 1 ? 'direct' : 'hls'))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, true)
    await vi.waitFor(() => expect(playback).toHaveBeenCalledTimes(1))
    const video = el.querySelector('video')!
    await vi.waitFor(() => expect(video.getAttribute('src')).toContain('/direct'))
    largeur(video, 0)
    video.dispatchEvent(new Event('loadedmetadata'))
    await vi.waitFor(() => expect(playback).toHaveBeenCalledTimes(2))
    expect(playback).toHaveBeenLastCalledWith('f', expect.objectContaining({ direct: false }))
    a.destroy()
  })

  it('garde la lecture directe quand l’image est bien décodée', async () => {
    const playback = vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux(1))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, true)
    const video = el.querySelector('video')!
    await vi.waitFor(() => expect(video.getAttribute('src')).toContain('/direct'))
    largeur(video, 1920)
    video.dispatchEvent(new Event('loadedmetadata'))
    await new Promise((r) => setTimeout(r, 20))
    expect(playback).toHaveBeenCalledTimes(1)
    a.destroy()
  })

  it('rouvre en HLS une lecture directe restée sans image après le délai d’attente', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    let n = 0
    const playback = vi.spyOn(filmsApi, 'playback').mockImplementation(async () => flux(++n, n === 1 ? 'direct' : 'hls'))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, true)
    const video = el.querySelector('video')!
    await vi.waitFor(() => expect(video.getAttribute('src')).toContain('/direct'))
    await vi.advanceTimersByTimeAsync(15_000)
    await vi.waitFor(() => expect(playback).toHaveBeenCalledTimes(2))
    expect(playback).toHaveBeenLastCalledWith('f', expect.objectContaining({ direct: false }))
    a.destroy()
  })

  it.each(['AbortError', 'NotSupportedError'])('ne passe pas en pause quand la lecture lancée est rejetée par %s (autre flux chargé ou repli)', async (nom) => {
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux(1))
    play.mockRejectedValueOnce(new DOMException('rejet', nom))
    const etats: PState[] = []
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: (s) => etats.push(s), onError: () => {} })
    a.load('f', 0, true)
    const video = el.querySelector('video')!
    await vi.waitFor(() => expect(video.getAttribute('src')).toContain('/direct'))
    largeur(video, 1920)
    video.dispatchEvent(new Event('loadedmetadata'))
    await new Promise((r) => setTimeout(r, 20))
    expect(play).toHaveBeenCalled()
    expect(etats).not.toContain('paused')
    a.destroy()
  })

  it('lit un flux HLS avec hls.js même quand le navigateur annonce un lecteur HLS natif', async () => {
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockImplementation((t: string) => (t === 'application/vnd.apple.mpegurl' ? 'maybe' : ''))
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux(1, 'hls'))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, true)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalledWith(expect.stringContaining('/hls.m3u8')))
    const video = el.querySelector('video')!
    expect(hlsjs.attachMedia).toHaveBeenCalledWith(video)
    expect(video.getAttribute('src') ?? '').not.toContain('.m3u8')
    a.destroy()
  })
})

describe('lecteur de films : diffusion sur une télé', () => {
  let prompt: ReturnType<typeof vi.fn>
  const distant = () => {
    const cible = new EventTarget()
    Object.assign(cible, { state: 'disconnected', prompt, watchAvailability: vi.fn().mockResolvedValue(1), cancelWatchAvailability: vi.fn().mockResolvedValue(undefined) })
    Object.defineProperty(HTMLMediaElement.prototype, 'remote', { configurable: true, get: () => cible })
    return cible
  }
  const ouvrir = async (mode: 'direct' | 'hls', onCast = vi.fn()) => {
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux(1, mode))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {}, onCast })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(mode === 'hls' ? hlsjs.loadSource : vi.mocked(HTMLMediaElement.prototype.load)).toHaveBeenCalled())
    return { a, video: el.querySelector('video')!, onCast }
  }
  beforeEach(() => {
    hlsjs.loadSource.mockClear()
    prompt = vi.fn().mockResolvedValue(undefined)
    vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    vi.spyOn(filmsApi, 'close').mockResolvedValue(undefined as never)
    vi.spyOn(filmsApi, 'progress').mockResolvedValue(undefined as never)
  })
  afterEach(() => { Reflect.deleteProperty(HTMLMediaElement.prototype, 'remote'); vi.restoreAllMocks() })

  it('ne propose rien sans l’API de diffusion du navigateur', async () => {
    const { a, onCast } = await ouvrir('direct')
    expect(a.cast).toBeUndefined()
    expect(onCast).not.toHaveBeenCalled()
    a.destroy()
  })

  it('annonce la diffusion puis suit l’état de la connexion', async () => {
    const cible = distant()
    const { a, onCast } = await ouvrir('direct')
    await vi.waitFor(() => expect(onCast).toHaveBeenLastCalledWith('disconnected'))
    Object.assign(cible, { state: 'connected' })
    cible.dispatchEvent(new Event('connect'))
    expect(onCast).toHaveBeenLastCalledWith('connected')
    a.destroy()
  })

  it('ne propose rien quand le navigateur a désactivé la diffusion', async () => {
    const cible = distant()
    Object.assign(cible, { watchAvailability: vi.fn().mockRejectedValue(new DOMException('désactivée', 'NotSupportedError')) })
    const { a, onCast } = await ouvrir('direct')
    await Promise.resolve()
    expect(onCast).not.toHaveBeenCalled()
    a.destroy()
  })

  it('ouvre le choix de l’appareil sur une lecture directe', async () => {
    distant()
    const { a } = await ouvrir('direct')
    expect(await a.cast!()).toBeNull()
    expect(prompt).toHaveBeenCalledTimes(1)
    a.destroy()
  })

  it('recharge un flux HLS dans le lecteur natif avant de proposer la diffusion', async () => {
    distant()
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockImplementation((t) => (t === 'application/vnd.apple.mpegurl' ? 'maybe' : ''))
    const { a, video } = await ouvrir('hls')
    expect(video.getAttribute('src')).toBeNull()
    const fini = a.cast!()
    await vi.waitFor(() => expect(video.getAttribute('src')).toContain('/stream/j1/hls.m3u8'))
    expect(prompt).not.toHaveBeenCalled()
    video.dispatchEvent(new Event('loadedmetadata'))
    expect(await fini).toBeNull()
    expect(prompt).toHaveBeenCalledTimes(1)
    a.destroy()
  })

  it('explique le refus quand le navigateur ne lit pas le HLS lui-même', async () => {
    distant()
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('')
    const { a } = await ouvrir('hls')
    expect(await a.cast!()).toBe(castRefusal('NotSupportedError'))
    expect(prompt).not.toHaveBeenCalled()
    a.destroy()
  })

  it('rend le message du refus du navigateur, rien si le choix est seulement fermé', async () => {
    distant()
    const { a } = await ouvrir('direct')
    prompt.mockRejectedValueOnce(new DOMException('aucun appareil', 'NotFoundError'))
    expect(await a.cast!()).toBe(castRefusal('NotFoundError'))
    prompt.mockRejectedValueOnce(new DOMException('fermé', 'NotAllowedError'))
    expect(await a.cast!()).toBeNull()
    a.destroy()
  })
})
