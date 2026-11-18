import { fireEvent, render } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import { tick } from 'svelte'
import { providers, type Adapter, type Handlers } from './providers'
import WatchTogether from './WatchTogether.svelte'

describe('salon cinéma : recharger le lecteur', () => {
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

  it('une intro connue ne fait ni bouton ni saut local : la position reste celle de la séance', async () => {
    vi.spyOn(api, 'watchReady').mockResolvedValue(undefined as never)
    const action = vi.spyOn(api, 'watchAction').mockResolvedValue(undefined as never)
    app.cinema.c1 = { state: { rev: 3, provider: 'film', video: 'f1', playing: true, pos: 45, rate: 1, at: new Date().toISOString(), host: 'u', host_only: false, by_id: 'u', by: 'u', act: 'play', queue: [], ready: {}, repeat: '', history: [] }, offset: 0 }
    const seek = vi.fn()
    let annoncer: Handlers['onTracks']
    vi.spyOn(providers, 'film').mockImplementation(async (_el, h) => {
      annoncer = h.onTracks
      return { sync: 'full', canRate: true, load: vi.fn(), play: vi.fn(), pause: vi.fn(), seek, time: () => 45, duration: () => 3000, rate: () => 1, setRate: vi.fn(), state: () => 'playing', destroy: vi.fn() } satisfies Adapter
    })
    const vue = render(WatchTogether, { channelId: 'c1', people: [], kind: 'cinema' })
    await vi.waitFor(() => expect(annoncer).toBeTruthy())
    annoncer!({ audio: [], subtitles: [], audioN: 0, subN: null, intro: { debut: 30, fin: 90 } })
    await tick()
    expect(vue.queryByRole('button', { name: /Passer l’intro/ })).toBeNull()
    expect(vue.container.textContent).not.toContain('Passer l’intro')
    await fireEvent.keyDown(document.body, { key: 's' })
    await fireEvent.keyDown(window, { key: 'S' })
    await tick()
    expect(seek.mock.calls.filter(([s]) => s === 90)).toEqual([])
    expect(action.mock.calls.filter(([, a]) => a === 'seek')).toEqual([])
    vue.unmount()
  })
})
