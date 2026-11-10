import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { API_URL, filmsApi } from '../../lib/api'
import type { FilmPlayback } from '../../sections/entracte/films'
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
  afterEach(() => vi.restoreAllMocks())

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
