import { fireEvent, render } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import { providers, type Adapter } from './providers'
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
})
