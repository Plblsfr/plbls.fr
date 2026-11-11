import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import type { FilmPlayback } from '../../sections/entracte/films'
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
