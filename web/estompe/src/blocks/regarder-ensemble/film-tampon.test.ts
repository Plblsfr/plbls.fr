import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import type { FilmPlayback } from '../../sections/entracte/films'
import { providers, TAMPON_FILM } from './providers'

const hlsjs = vi.hoisted(() => ({ config: vi.fn(), loadSource: vi.fn() }))
vi.mock('hls.js', () => ({
  default: class {
    static isSupported = () => true
    static Events = { ERROR: 'hlsError', MANIFEST_PARSED: 'hlsManifestParsed' }
    constructor(c: unknown) { hlsjs.config(c) }
    on() {}
    loadSource(u: string) { hlsjs.loadSource(u) }
    attachMedia() {}
    destroy() {}
  },
}))

const flux = (mode: 'direct' | 'hls', offset?: number): FilmPlayback => ({ stream_id: 's1', mode, url: `/stream/j/${mode === 'direct' ? 'direct' : 'hls.m3u8'}`, duration: 3000, audio: [], subtitles: [], offset })

function charge(video: HTMLVideoElement, plages: [number, number][]) {
  Object.defineProperty(video, 'buffered', { configurable: true, get: () => ({ length: plages.length, start: (i: number) => plages[i][0], end: (i: number) => plages[i][1] }) })
}

describe('lecteur de films : préchargement', () => {
  beforeEach(() => {
    vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined)
    vi.spyOn(filmsApi, 'close').mockResolvedValue(undefined as never)
    vi.spyOn(filmsApi, 'progress').mockResolvedValue(undefined as never)
  })
  afterEach(() => { vi.restoreAllMocks(); hlsjs.config.mockClear(); hlsjs.loadSource.mockClear() })

  it('demande au navigateur de charger la vidéo avant la lecture', async () => {
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('direct'))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    expect(el.querySelector('video')!.preload).toBe('auto')
    a.destroy()
  })

  it('annonce les plages chargées en lecture directe', async () => {
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('direct'))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    const video = el.querySelector('video')!
    await vi.waitFor(() => expect(video.getAttribute('src')).toContain('/direct'))
    expect(a.buffered!()).toEqual([])
    charge(video, [[0, 42], [600, 630]])
    expect(a.buffered!()).toEqual([[0, 42], [600, 630]])
    a.destroy()
  })

  it('ramène les plages d’un flux HLS au temps du titre et agrandit le tampon', async () => {
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 1200))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 1203, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalled())
    expect(hlsjs.config).toHaveBeenCalledWith(expect.objectContaining(TAMPON_FILM))
    expect(TAMPON_FILM.maxBufferLength).toBeGreaterThan(30)
    charge(el.querySelector('video')!, [[0, 64]])
    expect(a.buffered!()).toEqual([[1200, 1264]])
    a.destroy()
  })
})
