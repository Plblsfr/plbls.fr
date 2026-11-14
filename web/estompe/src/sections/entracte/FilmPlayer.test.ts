import { fireEvent, render } from '@testing-library/svelte'
import { tick } from 'svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import { providers, type Adapter, type Handlers } from '../../blocks/regarder-ensemble/providers'
import type { FilmDetail } from './films'
import FilmPlayer from './FilmPlayer.svelte'

const detail: FilmDetail = {
  id: 'f', kind: 'movie', title: 'La Combe', year: 2024, overview: null, duration: 3000, genres: [], season: null, episode: null,
  series_id: null, series_title: null, poster: false, backdrop: false, progress: null, audio: [], subtitles: [],
}

function monter() {
  const adapter = {
    sync: 'full', canRate: true, setVolume: vi.fn(), setAudio: vi.fn(), setSubtitle: vi.fn(), load: vi.fn(), play: vi.fn(), pause: vi.fn(),
    cast: vi.fn().mockResolvedValue(null), seek: vi.fn(), time: () => 0, duration: () => 3000, rate: () => 1, setRate: vi.fn(), state: () => 'paused', destroy: vi.fn(),
  } satisfies Adapter
  let handlers: Handlers | undefined
  vi.spyOn(filmsApi, 'item').mockResolvedValue(detail)
  vi.spyOn(providers, 'film').mockImplementation(async (_el, h) => { handlers = h; return adapter })
  const vue = render(FilmPlayer, { id: 'f' })
  return { adapter, vue, handlers: () => handlers! }
}

const pret = async (adapter: { load: ReturnType<typeof vi.fn> }) => vi.waitFor(() => expect(adapter.load).toHaveBeenCalled())

describe('FilmPlayer : son', () => {
  afterEach(() => vi.restoreAllMocks())

  it('applique le volume réglé au lecteur', async () => {
    const { adapter, vue } = monter()
    await pret(adapter)
    await fireEvent.input(vue.getByLabelText('Volume'), { target: { value: '40' } })
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(40)
  })

  it('coupe puis rétablit le son', async () => {
    const { adapter, vue } = monter()
    await pret(adapter)
    await fireEvent.click(vue.getByLabelText('Couper le son'))
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(0)
    await fireEvent.click(vue.getByLabelText('Rétablir le son'))
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(100)
  })

  it('rétablit un son audible après un volume mis à zéro', async () => {
    const { adapter, vue } = monter()
    await pret(adapter)
    await fireEvent.input(vue.getByLabelText('Volume'), { target: { value: '0' } })
    await fireEvent.click(vue.getByLabelText('Rétablir le son'))
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(100)
  })

  it('transmet le choix de la piste audio au lecteur', async () => {
    const { adapter, vue, handlers } = monter()
    await pret(adapter)
    const piste = (n: number, language: string) => ({ n, language, label: null })
    handlers().onTracks?.({ audio: [piste(0, 'fre'), piste(1, 'eng')], subtitles: [], audioN: 0, subN: null })
    await tick()
    await fireEvent.click(vue.getByText('Audio et sous-titres'))
    const boutons = vue.getAllByRole('button').filter((b) => b.classList.contains('film-track'))
    await fireEvent.click(boutons[1])
    expect(adapter.setAudio).toHaveBeenCalledWith(1)
  })
})

describe('FilmPlayer : diffusion sur une télé', () => {
  afterEach(() => vi.restoreAllMocks())

  it('ne montre le bouton que si le lecteur annonce la diffusion', async () => {
    const { adapter, vue, handlers } = monter()
    await pret(adapter)
    expect(vue.queryByLabelText('Diffuser sur une télé')).toBeNull()
    handlers().onCast?.('disconnected')
    await tick()
    await fireEvent.click(vue.getByLabelText('Diffuser sur une télé'))
    expect(adapter.cast).toHaveBeenCalledTimes(1)
  })

  it('signale la diffusion en cours', async () => {
    const { adapter, vue, handlers } = monter()
    await pret(adapter)
    handlers().onCast?.('connected')
    await tick()
    expect(vue.getByRole('status').textContent).toContain('Diffusion sur un autre appareil')
    expect(vue.getByLabelText('Diffusion en cours, changer d’appareil').getAttribute('aria-pressed')).toBe('true')
  })
})
