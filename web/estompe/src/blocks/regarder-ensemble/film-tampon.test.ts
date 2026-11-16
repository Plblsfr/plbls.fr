import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import type { FilmPlayback } from '../../sections/entracte/films'
import { providers, TAMPON_FILM } from './providers'

const hlsjs = vi.hoisted(() => ({ config: vi.fn(), loadSource: vi.fn(), on: {} as Record<string, (e: string, d: unknown) => void> }))
vi.mock('hls.js', () => ({
  default: class {
    static isSupported = () => true
    static Events = { ERROR: 'hlsError', MANIFEST_PARSED: 'hlsManifestParsed', LEVEL_UPDATED: 'hlsLevelUpdated' }
    static DefaultConfig = { loader: class {} }
    constructor(c: unknown) { hlsjs.config(c) }
    on(e: string, f: (e: string, d: unknown) => void) { hlsjs.on[e] = f }
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
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); localStorage.clear(); hlsjs.config.mockClear(); hlsjs.loadSource.mockClear(); hlsjs.on = {} })

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

  it('au réglage normal, laisse hls.js charger seul les segments', async () => {
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 0))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalled())
    expect(hlsjs.config.mock.calls[0][0].fLoader).toBeUndefined()
    a.destroy()
  })

  it('au réglage « tout le film », télécharge les segments en avance et les montre sur la barre', async () => {
    localStorage.setItem('entracte.lecture', '{"buffer":"all"}')
    const fetched: string[] = []
    vi.stubGlobal('fetch', vi.fn(async (u: string) => { fetched.push(u); return { ok: true, blob: async () => new Blob(['x']) } }))
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 1200))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 1200, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalled())
    expect(hlsjs.config.mock.calls[0][0].fLoader).toBeTypeOf('function')
    charge(el.querySelector('video')!, [[0, 6]])
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', { details: { fragments: [0, 1, 2, 3].map((i) => ({ url: `seg${i}.ts`, start: i * 4, duration: 4 })) } })
    await vi.waitFor(() => expect(fetched).toEqual(['seg0.ts', 'seg1.ts', 'seg2.ts', 'seg3.ts']))
    await vi.waitFor(() => expect(a.buffered!()).toEqual([[1200, 1216]]))
    a.destroy()
  })

  it('changer le réglage rouvre le flux transcodé à la position en cours', async () => {
    const playback = vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 0))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalledTimes(1))
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, blob: async () => new Blob(['x']) })))
    a.setBuffer!('long')
    await vi.waitFor(() => expect(playback).toHaveBeenCalledTimes(2))
    await vi.waitFor(() => expect(hlsjs.config.mock.calls.at(-1)![0].fLoader).toBeTypeOf('function'))
    expect(JSON.parse(localStorage.getItem('entracte.lecture')!).buffer).toBe('long')
    a.destroy()
  })
})
