import { fireEvent, render } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import { tick } from 'svelte'
import { providers, type Adapter, type Handlers } from './providers'
import WatchTogether from './WatchTogether.svelte'

describe('salon cinéma : recharger le lecteur', () => {
  beforeEach(() => { vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} }) })
  afterEach(() => { vi.restoreAllMocks(); delete app.cinema.c1 })

  it('recrée le lecteur et recharge le film à la position de la séance', async () => {
    vi.spyOn(api, 'watchReady').mockResolvedValue(undefined as never)
    app.cinema.c1 = { state: { rev: 3, provider: 'film', video: 'f1', playing: false, pos: 600, rate: 1, at: new Date().toISOString(), host: 'u', host_only: false, by_id: 'u', by: 'u', act: 'pause', queue: [], ready: {}, repeat: '', history: [] }, offset: 0 }
    const adapters: { load: ReturnType<typeof vi.fn>; destroy: ReturnType<typeof vi.fn> }[] = []
    vi.spyOn(providers, 'film').mockImplementation(async () => {
      const a = { sync: 'full', canRate: true, load: vi.fn(), play: vi.fn(), pause: vi.fn(), seek: vi.fn(), time: () => 600, duration: () => 3000, rate: () => 1, setRate: vi.fn(), state: () => 'paused', destroy: vi.fn() } satisfies Adapter
      adapters.push(a)
      return a
    })
    const vue = render(WatchTogether, { channelId: 'c1', people: [], kind: 'cinema' })
    await vi.waitFor(() => expect(adapters[0]?.load).toHaveBeenCalledWith('f1', 600, false))
    await fireEvent.click(vue.getByRole('button', { name: 'Recharger' }))
    await vi.waitFor(() => expect(adapters[1]?.load).toHaveBeenCalledWith('f1', 600, false))
    expect(adapters[0].destroy).toHaveBeenCalled()
    vue.unmount()
    expect(adapters[1].destroy).toHaveBeenCalled()
  })

  const seance = (extra: Record<string, unknown> = {}) => {
    app.cinema.c1 = { state: { rev: 3, provider: 'film', video: 'f1', playing: true, pos: 45, rate: 1, at: new Date().toISOString(), host: 'u', host_only: false, by_id: 'u', by: 'u', act: 'play', queue: [], ready: {}, repeat: '', history: [], ...extra }, offset: 0 }
    const seek = vi.fn()
    const load = vi.fn()
    const pret: { annoncer?: Handlers['onTracks']; observer?: Handlers['onState'] } = {}
    vi.spyOn(providers, 'film').mockImplementation(async (_el, h) => {
      pret.annoncer = h.onTracks
      pret.observer = h.onState
      return { sync: 'full', canRate: true, load, play: vi.fn(), pause: vi.fn(), seek, time: () => 45, duration: () => 3000, rate: () => 1, setRate: vi.fn(), state: () => 'playing', destroy: vi.fn() } satisfies Adapter
    })
    return { seek, load, pret }
  }

  it('« Passer l’intro » déplace la séance de tous à la fin de l’intro, au bouton comme à la touche S', async () => {
    vi.spyOn(api, 'watchReady').mockResolvedValue(undefined as never)
    const action = vi.spyOn(api, 'watchAction').mockResolvedValue(undefined as never)
    const requete = vi.fn(async (..._a: unknown[]) => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', requete)
    const { seek, load, pret } = seance()
    const vue = render(WatchTogether, { channelId: 'c1', people: [], kind: 'cinema' })
    await vi.waitFor(() => expect(load).toHaveBeenCalled())
    expect(vue.queryByRole('button', { name: /Passer l’intro/ })).toBeNull()
    pret.annoncer!({ audio: [], subtitles: [], audioN: 0, subN: null, intro: { debut: 30, fin: 90 } })
    pret.observer!('playing')
    await fireEvent.click(await vue.findByRole('button', { name: /Passer l’intro/ }))
    expect(seek.mock.calls).toEqual([[90]])
    await vue.findByRole('button', { name: /Passer l’intro/ })
    await fireEvent.keyDown(document.body, { key: 's' })
    expect(seek.mock.calls).toEqual([[90], [90]])
    await vi.waitFor(() => expect(action).toHaveBeenCalledWith('c1', 'seek', expect.objectContaining({ position: 90, video: 'f1' }), 'cinema'))
    expect(requete.mock.calls.some(([url]) => String(url).endsWith('/titles/f1/intro-skipped'))).toBe(true)
    vue.unmount()
    vi.unstubAllGlobals()
  })

  it('sans le droit de piloter, une intro connue ne fait ni bouton ni saut', async () => {
    vi.spyOn(api, 'watchReady').mockResolvedValue(undefined as never)
    const action = vi.spyOn(api, 'watchAction').mockResolvedValue(undefined as never)
    const { seek, load, pret } = seance({ host: 'autre', host_only: true })
    const vue = render(WatchTogether, { channelId: 'c1', people: [], kind: 'cinema' })
    await vi.waitFor(() => expect(load).toHaveBeenCalled())
    pret.annoncer!({ audio: [], subtitles: [], audioN: 0, subN: null, intro: { debut: 30, fin: 90 } })
    pret.observer!('playing')
    await vi.waitFor(() => expect(vue.getByText('0:45 / 50:00')).toBeTruthy())
    expect(vue.queryByRole('button', { name: /Passer l’intro/ })).toBeNull()
    await fireEvent.keyDown(document.body, { key: 's' })
    await tick()
    expect(seek.mock.calls.filter(([s]) => s === 90)).toEqual([])
    expect(action.mock.calls.filter(([, a]) => a === 'seek')).toEqual([])
    vue.unmount()
  })
})
