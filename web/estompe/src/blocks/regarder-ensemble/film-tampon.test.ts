import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import type { FilmPlayback } from '../../sections/entracte/films'
import { setPlatform } from '../../lib/platform'
import { platform as web } from '@platform'
import { providers, TAMPON_FILM } from './providers'

const hlsjs = vi.hoisted(() => ({ config: vi.fn(), loadSource: vi.fn(), innerLoad: vi.fn(), on: {} as Record<string, (e: string, d: unknown) => void> }))
vi.mock('hls.js', () => ({
  default: class {
    static isSupported = () => true
    static Events = { ERROR: 'hlsError', MANIFEST_PARSED: 'hlsManifestParsed', LEVEL_UPDATED: 'hlsLevelUpdated' }
    static DefaultConfig = { loader: class { stats = { loading: {} }; load(c: unknown) { hlsjs.innerLoad(c) } abort() {} destroy() {} } }
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
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); localStorage.clear(); hlsjs.config.mockClear(); hlsjs.loadSource.mockClear(); hlsjs.innerLoad.mockClear(); hlsjs.on = {} })

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

  const segments = (n: number, jeton = 'j') => ({ details: { fragments: Array.from({ length: n }, (_, i) => ({ url: `/stream/${jeton}/seg${i}.ts`, start: i * 4, duration: 4 })) } })
  const reseau = (ok: () => boolean) => {
    const fetched: string[] = []
    vi.stubGlobal('fetch', vi.fn(async (u: string) => { fetched.push(u); return ok() ? { ok: true, status: 200, blob: async () => new Blob(['x']) } : { ok: false, status: 404 } }))
    return fetched
  }
  type Chargeur = new (c: unknown) => { load(c: unknown, o: unknown, cb: { onSuccess: (r: { url: string; data: ArrayBuffer }) => void }): void }

  it('au réglage normal, laisse hls.js charger seul les segments et ne précharge rien', async () => {
    const fetched = reseau(() => true)
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 0))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalled())
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(4))
    const Loader = hlsjs.config.mock.calls[0][0].fLoader as Chargeur
    new Loader({}).load({ url: '/stream/j/seg0.ts' }, {}, { onSuccess: () => {} })
    expect(hlsjs.innerLoad).toHaveBeenCalledWith({ url: '/stream/j/seg0.ts' })
    expect(fetched).toEqual([])
    expect(a.buffered!()).toEqual([])
    a.destroy()
  })

  it('au réglage « tout le film », télécharge les segments en avance et les montre sur la barre', async () => {
    localStorage.setItem('entracte.lecture', '{"buffer":"all"}')
    const fetched = reseau(() => true)
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 1200))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 1200, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalled())
    expect(hlsjs.config.mock.calls[0][0].fLoader).toBeTypeOf('function')
    charge(el.querySelector('video')!, [[0, 6]])
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(4))
    await vi.waitFor(() => expect(fetched).toEqual([0, 1, 2, 3].map((i) => `/stream/j/seg${i}.ts?avance=1`)))
    await vi.waitFor(() => expect(a.buffered!()).toEqual([[1200, 1216]]))
    a.destroy()
  })

  it('changer le réglage lance le préchargement sans rouvrir le flux', async () => {
    const playback = vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 0))
    const fetched = reseau(() => true)
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalledTimes(1))
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(3))
    expect(fetched).toEqual([])
    a.setBuffer!('long')
    await vi.waitFor(() => expect(fetched).toHaveLength(3))
    expect(JSON.parse(localStorage.getItem('entracte.lecture')!).buffer).toBe('long')
    expect(playback).toHaveBeenCalledTimes(1)
    expect(hlsjs.loadSource).toHaveBeenCalledTimes(1)
    a.setBuffer!('normal')
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(6))
    const Loader = hlsjs.config.mock.calls[0][0].fLoader as Chargeur
    const recu = await new Promise<number>((ok) => new Loader({}).load({ url: '/stream/j/seg1.ts' }, {}, { onSuccess: (r) => ok(r.data.byteLength) }))
    expect(recu).toBe(1)
    expect(fetched).toHaveLength(3)
    expect(hlsjs.innerLoad).not.toHaveBeenCalled()
    a.destroy()
  })

  it('garde ce qui est chargé quand le flux est rouvert avec la même qualité et la même piste, et repart de zéro sinon', async () => {
    localStorage.setItem('entracte.lecture', '{"buffer":"all"}')
    const fetched = reseau(() => true)
    let n = 0
    let cle = 'f-480p-0-g'
    const playback = vi.spyOn(filmsApi, 'playback').mockImplementation(async () => ({ ...flux('hls', 0), stream_id: `s${++n}`, url: `/stream/j${n}/hls.m3u8`, cache: cle }))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalledTimes(1))
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(3, 'j1'))
    await vi.waitFor(() => expect(fetched).toHaveLength(3))

    a.setAudio!(0)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalledTimes(2))
    expect(playback).toHaveBeenCalledTimes(2)
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(4, 'j2'))
    await vi.waitFor(() => expect(fetched).toEqual([0, 1, 2].map((i) => `/stream/j1/seg${i}.ts?avance=1`).concat('/stream/j2/seg3.ts?avance=1')))
    await vi.waitFor(() => expect(a.buffered!()).toEqual([[0, 16]]))

    cle = 'f-720p-0-g'
    a.setQuality!('720p')
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalledTimes(3))
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(2, 'j3'))
    await vi.waitFor(() => expect(fetched.slice(4)).toEqual(['/stream/j3/seg0.ts?avance=1', '/stream/j3/seg1.ts?avance=1']))
    await vi.waitFor(() => expect(a.buffered!()).toEqual([[0, 8]]))
    a.destroy()
  })

  it('en « Wi-Fi seulement », ne précharge pas en données mobiles et reprend seul au retour du Wi-Fi', async () => {
    localStorage.setItem('entracte.lecture', '{"buffer":"all","wifiOnly":true}')
    const fetched = reseau(() => true)
    let type: 'wifi' | 'cellular' = 'cellular'
    let change = () => {}
    setPlatform({ ...web, network: { kind: () => type, watch: (f) => { change = f; return () => { change = () => {} } } } })
    vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux('hls', 0))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 0, false)
    await vi.waitFor(() => expect(hlsjs.loadSource).toHaveBeenCalled())
    hlsjs.on.hlsLevelUpdated('hlsLevelUpdated', segments(3))
    el.querySelector('video')!.dispatchEvent(new Event('timeupdate'))
    expect(fetched).toEqual([])
    type = 'wifi'
    change()
    await vi.waitFor(() => expect(fetched).toHaveLength(3))
    a.setWifiOnly!(false)
    expect(JSON.parse(localStorage.getItem('entracte.lecture')!).wifiOnly).toBe(false)
    a.destroy()
    setPlatform(web)
  })
})
