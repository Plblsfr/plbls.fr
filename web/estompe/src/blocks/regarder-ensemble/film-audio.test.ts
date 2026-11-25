import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { API_URL, filmsApi } from '../../lib/api'
import type { FilmDetail, FilmPlayback } from '../../sections/entracte/films'
import { providers } from './providers'

const flux = (n: number): FilmPlayback => ({ stream_id: `s${n}`, mode: 'direct', url: `/films/stream/j${n}/direct`, duration: 3000, audio: [], subtitles: [] })

describe('lecteur de films : son', () => {
  beforeEach(() => {
    vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    vi.spyOn(filmsApi, 'close').mockResolvedValue(undefined as never)
    vi.spyOn(filmsApi, 'progress').mockResolvedValue(undefined as never)
  })
  afterEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  it('rouvre le flux avec la piste choisie à la position demandée, même avant le chargement', async () => {
    let n = 0
    const playback = vi.spyOn(filmsApi, 'playback').mockImplementation(async () => flux(++n))
    const el = document.createElement('div')
    const a = await providers.film(el, { onState: () => {}, onError: () => {} })
    a.load('f', 600, true)
    await vi.waitFor(() => expect(playback).toHaveBeenCalledTimes(1))
    a.setAudio?.(1)
    await vi.waitFor(() => expect(playback).toHaveBeenCalledTimes(2))
    expect(playback).toHaveBeenLastCalledWith('f', expect.objectContaining({ audio: 1 }))
    const video = el.querySelector('video')!
    await vi.waitFor(() => expect(video.getAttribute('src')).toBe(API_URL + flux(2).url))
    video.dispatchEvent(new Event('loadedmetadata'))
    expect(video.currentTime).toBe(600)
    expect(HTMLMediaElement.prototype.play).toHaveBeenCalled()
    a.destroy()
  })

  it('ouvre le titre avec la langue et les sous-titres choisis par défaut', async () => {
    localStorage.setItem('entracte.lecture', JSON.stringify({ audio: 'vff', subtitles: 'forced' }))
    const audio = [{ n: 0, label: 'English', language: 'eng' }, { n: 1, label: 'VFF', language: 'fre' }]
    const subtitles = [{ n: 0, label: 'English', language: 'eng', url: '/s/0' }, { n: 1, label: 'Forcés', language: 'fre', forced: true, url: '/s/1' }]
    vi.spyOn(filmsApi, 'item').mockResolvedValue({ audio, subtitles } as unknown as FilmDetail)
    const playback = vi.spyOn(filmsApi, 'playback').mockResolvedValue({ ...flux(1), audio, subtitles })
    const pistes = vi.fn()
    const a = await providers.film(document.createElement('div'), { onState: () => {}, onError: () => {}, onTracks: pistes })
    a.load('f', 0, true)
    await vi.waitFor(() => expect(pistes).toHaveBeenCalled())
    expect(playback).toHaveBeenCalledTimes(1)
    expect(playback).toHaveBeenCalledWith('f', expect.objectContaining({ audio: 1 }))
    expect(pistes).toHaveBeenLastCalledWith(expect.objectContaining({ audioN: 1, subN: 1 }))
    a.destroy()
  })

  it('rétablit les sous-titres choisis quand le navigateur active une autre piste', async () => {
    localStorage.setItem('entracte.lecture', JSON.stringify({ subtitles: 'forced' }))
    const audio = [{ n: 0, label: 'VFF', language: 'fre' }]
    const subtitles = [{ n: 0, label: 'Français', language: 'fre', url: '/s/0' }, { n: 1, label: 'Forcés', language: 'fre', forced: true, url: '/s/1' }]
    vi.spyOn(filmsApi, 'playback').mockResolvedValue({ ...flux(1), audio, subtitles })
    const liste = Object.assign(new EventTarget(), { 0: { mode: 'disabled' }, 1: { mode: 'disabled' }, length: 2 })
    vi.spyOn(HTMLMediaElement.prototype, 'textTracks', 'get').mockReturnValue(liste as unknown as TextTrackList)
    const pistes = vi.fn()
    const a = await providers.film(document.createElement('div'), { onState: () => {}, onError: () => {}, onTracks: pistes })
    a.load('f', 0, true)
    await vi.waitFor(() => expect(pistes).toHaveBeenCalled())
    expect([liste[0].mode, liste[1].mode]).toEqual(['disabled', 'showing'])
    liste[0].mode = 'showing'
    liste.dispatchEvent(new Event('change'))
    expect([liste[0].mode, liste[1].mode]).toEqual(['disabled', 'showing'])
    a.setSubtitle?.(null)
    liste[0].mode = 'showing'
    liste.dispatchEvent(new Event('change'))
    expect([liste[0].mode, liste[1].mode]).toEqual(['disabled', 'disabled'])
    a.destroy()
  })

  it('garde la piste choisie à la main pendant l’ouverture', async () => {
    localStorage.setItem('entracte.lecture', JSON.stringify({ audio: 'vff' }))
    vi.spyOn(filmsApi, 'item').mockResolvedValue({ audio: [{ n: 0, label: null, language: 'eng' }, { n: 1, label: 'VFF', language: 'fre' }, { n: 2, label: null, language: 'jpn' }] } as unknown as FilmDetail)
    const playback = vi.spyOn(filmsApi, 'playback').mockResolvedValue(flux(1))
    const a = await providers.film(document.createElement('div'), { onState: () => {}, onError: () => {} })
    a.load('f', 0, true)
    a.setAudio?.(2)
    await vi.waitFor(() => expect(playback).toHaveBeenCalled())
    expect(playback).toHaveBeenLastCalledWith('f', expect.objectContaining({ audio: 2 }))
    a.destroy()
  })

  it('coupe vraiment le son à zéro, y compris là où le volume est imposé par l’appareil', async () => {
    const el = document.createElement('div')
    const a = await providers.file(el, { onState: () => {}, onError: () => {} })
    const video = el.querySelector('video')!
    a.setVolume?.(0)
    expect(video.muted).toBe(true)
    a.setVolume?.(30)
    expect(video.muted).toBe(false)
    expect(video.volume).toBeCloseTo(0.3)
    a.destroy()
  })
})
