import { fireEvent, render } from '@testing-library/svelte'
import { tick } from 'svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import { providers, type Adapter, type Handlers } from '../../blocks/regarder-ensemble/providers'
import { IDLE_MS, type FilmDetail } from './films'
import FilmPlayer from './FilmPlayer.svelte'

const detail: FilmDetail = {
  id: 'f', kind: 'movie', title: 'La Combe', year: 2024, overview: null, duration: 3000, genres: [], season: null, episode: null,
  series_id: null, series_title: null, poster: false, backdrop: false, progress: null, audio: [], subtitles: [],
}

function monter() {
  const adapter = {
    sync: 'full', canRate: true, setVolume: vi.fn(), setAudio: vi.fn(), setSubtitle: vi.fn(), load: vi.fn(), play: vi.fn(), pause: vi.fn(),
    seek: vi.fn(), time: () => 0, duration: () => 3000, rate: () => 1, setRate: vi.fn(), state: () => 'paused', destroy: vi.fn(),
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

describe('FilmPlayer : commandes masquées sans geste', () => {
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks() })

  const lecteur = async () => {
    const m = monter()
    await pret(m.adapter)
    vi.useFakeTimers()
    return { ...m, racine: m.vue.container.querySelector('.film-player')! }
  }

  it('masque les commandes pendant la lecture, un geste les ramène', async () => {
    const { handlers, racine } = await lecteur()
    handlers().onState('playing')
    await tick()
    expect(racine.classList.contains('idle')).toBe(false)
    await vi.advanceTimersByTimeAsync(IDLE_MS)
    expect(racine.classList.contains('idle')).toBe(true)
    await fireEvent.pointerMove(racine)
    expect(racine.classList.contains('idle')).toBe(false)
    await vi.advanceTimersByTimeAsync(IDLE_MS)
    expect(racine.classList.contains('idle')).toBe(true)
  })

  it('les garde en pause', async () => {
    const { handlers, racine } = await lecteur()
    handlers().onState('paused')
    await tick()
    await vi.advanceTimersByTimeAsync(IDLE_MS * 2)
    expect(racine.classList.contains('idle')).toBe(false)
  })

  it('les ramène quand la lecture s’arrête et les garde tant que le choix des pistes est ouvert', async () => {
    const { handlers, racine, vue } = await lecteur()
    handlers().onState('playing')
    await tick()
    await vi.advanceTimersByTimeAsync(IDLE_MS)
    expect(racine.classList.contains('idle')).toBe(true)
    handlers().onState('paused')
    await tick()
    expect(racine.classList.contains('idle')).toBe(false)
    handlers().onState('playing')
    handlers().onTracks?.({ audio: [], subtitles: [], audioN: 0, subN: null })
    await tick()
    await fireEvent.click(vue.getByText('Audio et sous-titres'))
    await vi.advanceTimersByTimeAsync(IDLE_MS * 2)
    expect(racine.classList.contains('idle')).toBe(false)
  })
})
