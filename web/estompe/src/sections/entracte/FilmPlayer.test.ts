import { fireEvent, render } from '@testing-library/svelte'
import { tick } from 'svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as api from '../../lib/api'
import { filmsApi } from '../../lib/api'
import { providers, type Adapter, type Handlers } from '../../blocks/regarder-ensemble/providers'
import { IDLE_MS, type FilmDetail } from './films'
import FilmPlayer from './FilmPlayer.svelte'
import { medias } from '../../lib/medias'
import { setPlatform } from '../../lib/platform'
import { platform as web } from '@platform'

const detail: FilmDetail = {
  id: 'f', kind: 'movie', title: 'La Combe', year: 2024, overview: null, duration: 3000, genres: [], season: null, episode: null,
  series_id: null, series_title: null, poster: false, backdrop: false, progress: null, audio: [], subtitles: [],
}

function monter(position = { t: 0 }) {
  const adapter = {
    sync: 'full', canRate: true, setVolume: vi.fn(), setAudio: vi.fn(), setSubtitle: vi.fn(), load: vi.fn(), play: vi.fn(), pause: vi.fn(),
    cast: vi.fn().mockResolvedValue(null), seek: vi.fn((x: number) => { position.t = x }), time: () => position.t, duration: () => 3000, rate: () => 1, setRate: vi.fn(), state: () => 'paused', destroy: vi.fn(),
  } satisfies Adapter
  let handlers: Handlers | undefined
  vi.spyOn(filmsApi, 'item').mockResolvedValue(detail)
  vi.spyOn(providers, 'film').mockImplementation(async (_el, h) => { handlers = h; return adapter })
  const vue = render(FilmPlayer, { id: 'f' })
  return { adapter, vue, handlers: () => handlers! }
}

const pret = async (adapter: { load: ReturnType<typeof vi.fn> }) => vi.waitFor(() => expect(adapter.load).toHaveBeenCalled())

describe('FilmPlayer : son', () => {
  afterEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  it('reprend le volume réglé à la lecture précédente', async () => {
    const premier = monter()
    await pret(premier.adapter)
    await fireEvent.input(premier.vue.getByLabelText('Volume'), { target: { value: '40' } })
    premier.vue.unmount()
    const { adapter, vue } = monter()
    await pret(adapter)
    await tick()
    expect((vue.getByLabelText('Volume') as HTMLInputElement).value).toBe('40')
    expect(adapter.setVolume).toHaveBeenLastCalledWith(40)
  })

  it('reste muet si le son était coupé', async () => {
    const premier = monter()
    await pret(premier.adapter)
    await fireEvent.click(premier.vue.getByLabelText('Couper le son'))
    premier.vue.unmount()
    const { adapter, vue } = monter()
    await pret(adapter)
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(0)
    await fireEvent.click(vue.getByLabelText('Rétablir le son'))
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(100)
  })

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

describe('FilmPlayer : pistes par défaut', () => {
  afterEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  const piste = (n: number, language: string, label: string | null = null, forced = false) => ({ n, language, label, forced })
  const ouvrir = async () => {
    const m = monter()
    await pret(m.adapter)
    m.handlers().onTracks?.({ audio: [piste(0, 'eng'), piste(1, 'fre', 'VFF')], subtitles: [piste(0, 'eng'), piste(1, 'fre', 'Forcés', true)], audioN: 0, subN: null })
    await tick()
    await fireEvent.click(m.vue.getByText('Audio et sous-titres'))
    return m
  }

  it('applique la langue choisie au titre en cours et la retient', async () => {
    const { adapter, vue } = await ouvrir()
    await fireEvent.change(vue.getByLabelText('Audio'), { target: { value: 'vff' } })
    expect(adapter.setAudio).toHaveBeenCalledWith(1)
    expect(JSON.parse(localStorage.getItem('entracte.lecture')!).audio).toBe('vff')
  })

  it('applique les sous-titres forcés de la langue écoutée et les retient', async () => {
    const { adapter, vue, handlers } = await ouvrir()
    await fireEvent.change(vue.getByLabelText('Sous-titres'), { target: { value: 'forced' } })
    expect(adapter.setSubtitle).toHaveBeenLastCalledWith(null)
    handlers().onTracks?.({ audio: [piste(0, 'eng'), piste(1, 'fre', 'VFF')], subtitles: [piste(0, 'eng'), piste(1, 'fre', 'Forcés', true)], audioN: 1, subN: null })
    await tick()
    await fireEvent.change(vue.getByLabelText('Sous-titres'), { target: { value: 'forced' } })
    expect(adapter.setSubtitle).toHaveBeenLastCalledWith(1)
    expect(JSON.parse(localStorage.getItem('entracte.lecture')!).subtitles).toBe('forced')
  })
  it('fait suivre les sous-titres forcés quand la langue par défaut change', async () => {
    const { adapter, vue } = await ouvrir()
    await fireEvent.change(vue.getByLabelText('Sous-titres'), { target: { value: 'forced' } })
    expect(adapter.setSubtitle).toHaveBeenLastCalledWith(null)
    await fireEvent.change(vue.getByLabelText('Audio'), { target: { value: 'vff' } })
    expect(adapter.setSubtitle).toHaveBeenLastCalledWith(1)
    expect(adapter.setAudio).toHaveBeenCalledWith(1)
  })
  it('choisit les sous-titres forcés de la langue demandée sans attendre la réouverture du flux', async () => {
    const { adapter, vue } = await ouvrir()
    await fireEvent.change(vue.getByLabelText('Audio'), { target: { value: 'vff' } })
    await fireEvent.change(vue.getByLabelText('Sous-titres'), { target: { value: 'forced' } })
    expect(adapter.setSubtitle).toHaveBeenLastCalledWith(1)
  })
})

describe('FilmPlayer : préchargement réservé au Wi-Fi', () => {
  afterEach(() => { localStorage.clear(); vi.restoreAllMocks(); setPlatform(web) })

  const ouvrir = async (kind?: 'wifi' | 'cellular' | 'unknown') => {
    setPlatform({ ...web, network: kind ? { kind: () => kind, watch: () => () => {} } : undefined })
    const m = monter()
    const setWifiOnly = vi.fn()
    Object.assign(m.adapter, { setWifiOnly })
    await pret(m.adapter)
    m.handlers().onTracks?.({ audio: [{ n: 0, language: 'fre', label: null }], subtitles: [], audioN: 0, subN: null })
    await tick()
    await fireEvent.click(m.vue.getByText('Audio et sous-titres'))
    return { ...m, setWifiOnly }
  }

  it('n’affiche pas l’option quand l’appareil ne sait pas dire s’il est en Wi-Fi', async () => {
    for (const kind of [undefined, 'unknown'] as const) {
      const { vue } = await ouvrir(kind)
      expect(vue.getByLabelText('Préchargement')).toBeTruthy()
      expect(vue.queryByLabelText(/Wi-Fi seulement/)).toBeNull()
      vue.unmount()
    }
  })

  it('affiche l’option quand le type de connexion est connu, l’applique et la retient', async () => {
    const { vue, setWifiOnly } = await ouvrir('cellular')
    const option = vue.getByLabelText(/Wi-Fi seulement/) as HTMLInputElement
    expect(option.checked).toBe(false)
    await fireEvent.click(option)
    expect(setWifiOnly).toHaveBeenCalledWith(true)
    expect(JSON.parse(localStorage.getItem('entracte.lecture')!).wifiOnly).toBe(true)
  })
})

describe('FilmPlayer : clavier', () => {
  afterEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  it('met en pause et relance avec Espace', async () => {
    const { adapter, handlers } = monter()
    await pret(adapter)
    await fireEvent.keyDown(document.body, { key: ' ' })
    expect(adapter.play).toHaveBeenCalledTimes(1)
    handlers().onState('playing')
    await tick()
    await fireEvent.keyDown(document.body, { key: 'k' })
    expect(adapter.pause).toHaveBeenCalledTimes(1)
  })

  it('avance, recule et règle le son', async () => {
    const { adapter, vue } = monter()
    await pret(adapter)
    await fireEvent.keyDown(document.body, { key: 'ArrowRight' })
    expect(adapter.seek).toHaveBeenLastCalledWith(10)
    await fireEvent.keyDown(document.body, { key: 'ArrowLeft' })
    expect(adapter.seek).toHaveBeenLastCalledWith(0)
    await fireEvent.keyDown(document.body, { key: 'ArrowDown' })
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(95)
    await fireEvent.keyDown(document.body, { key: 'm' })
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(0)
    await fireEvent.keyDown(document.body, { key: 'ArrowUp' })
    await tick()
    expect(adapter.setVolume).toHaveBeenLastCalledWith(5)
    expect(vue.getByLabelText('Couper le son')).toBeTruthy()
  })

  it('bascule le plein écran avec F', async () => {
    const { adapter, vue } = monter()
    await pret(adapter)
    const racine = vue.container.querySelector('.film-player') as HTMLElement
    racine.requestFullscreen = vi.fn().mockResolvedValue(undefined)
    await fireEvent.keyDown(document.body, { key: 'f' })
    expect(racine.requestFullscreen).toHaveBeenCalledTimes(1)
  })

  it('ne réagit ni aux combinaisons ni depuis une liste de choix', async () => {
    const { adapter, vue, handlers } = monter()
    await pret(adapter)
    await fireEvent.keyDown(document.body, { key: 'k', ctrlKey: true })
    handlers().onTracks?.({ audio: [], subtitles: [], audioN: 0, subN: null })
    await tick()
    await fireEvent.click(vue.getByText('Audio et sous-titres'))
    await fireEvent.keyDown(vue.getByLabelText('Audio'), { key: ' ' })
    expect(adapter.play).not.toHaveBeenCalled()
  })

  it('laisse Espace à la commande atteinte au clavier', async () => {
    const { adapter, vue } = monter()
    await pret(adapter)
    const bouton = vue.getByLabelText('Couper le son')
    vi.spyOn(bouton, 'matches').mockImplementation((s) => s === ':focus-visible' || s === 'button, a, input')
    const libre = await fireEvent.keyDown(bouton, { key: ' ' })
    expect(libre).toBe(true)
    expect(adapter.play).not.toHaveBeenCalled()
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

  it('les garde pendant une diffusion sur une télé', async () => {
    const { handlers, racine } = await lecteur()
    handlers().onState('playing')
    handlers().onCast?.('connected')
    await tick()
    await vi.advanceTimersByTimeAsync(IDLE_MS * 2)
    expect(racine.classList.contains('idle')).toBe(false)
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

describe('FilmPlayer : autres médias', () => {
  afterEach(() => { localStorage.clear(); vi.restoreAllMocks() })

  it('le démarrage du film est annoncé une fois (la musique de Sillon se met en pause), pas à chaque reprise après mise en mémoire tampon', async () => {
    const demarre = vi.spyOn(medias, 'demarre')
    const { adapter, handlers } = monter()
    await pret(adapter)
    handlers().onState('playing')
    handlers().onState('buffering')
    handlers().onState('playing')
    expect(demarre.mock.calls).toEqual([['film']])
    handlers().onState('paused')
    handlers().onState('playing')
    expect(demarre.mock.calls).toEqual([['film'], ['film']])
  })
})

describe('FilmPlayer : passer l’intro', () => {
  afterEach(() => { vi.useRealTimers(); localStorage.clear(); vi.restoreAllMocks() })

  const pistes = (intro: { debut: number; fin: number } | null) => ({ audio: [], subtitles: [], audioN: 0, subN: null, intro })
  async function ouvert(intro: { debut: number; fin: number } | null, t: number) {
    vi.useFakeTimers()
    const signale = vi.spyOn(api, 'requestAt').mockRejectedValue(new Error('hors ligne'))
    const position = { t }
    const m = monter(position)
    await vi.waitFor(() => expect(m.adapter.load).toHaveBeenCalled())
    m.handlers().onTracks!(pistes(intro))
    const avancer = async (x: number) => { position.t = x; await vi.advanceTimersByTimeAsync(600) }
    await avancer(t)
    return { ...m, avancer, signale }
  }
  const bouton = (vue: ReturnType<typeof monter>['vue']) => vue.queryByRole('button', { name: 'Passer l’intro' })

  it('n’apparaît que dans [debut, fin)', async () => {
    const { vue, avancer } = await ouvert({ debut: 30, fin: 90 }, 10)
    expect(bouton(vue)).toBeNull()
    await avancer(30)
    expect(bouton(vue)).toBeTruthy()
    await avancer(89.5)
    expect(bouton(vue)).toBeTruthy()
    await avancer(90)
    expect(bouton(vue)).toBeNull()
  })

  it('un clic saute à la fin de l’intro, le signale à Entracte et le bouton disparaît', async () => {
    const { vue, adapter, signale } = await ouvert({ debut: 30, fin: 90 }, 45)
    await fireEvent.click(bouton(vue)!)
    expect(adapter.seek).toHaveBeenLastCalledWith(90)
    expect(signale.mock.calls).toEqual([[filmsApi.base, 'POST', '/titles/f/intro-skipped']])
    await tick()
    expect(bouton(vue)).toBeNull()
    await fireEvent.keyDown(document.body, { key: 's' })
    expect(signale).toHaveBeenCalledTimes(1)
  })

  it('la touche S saute l’intro, et ne fait rien hors de l’intro', async () => {
    const { vue, adapter, avancer, signale } = await ouvert({ debut: 30, fin: 90 }, 10)
    await fireEvent.keyDown(document.body, { key: 's' })
    expect(adapter.seek).not.toHaveBeenCalled()
    expect(signale).not.toHaveBeenCalled()
    await avancer(31)
    expect(bouton(vue)!.getAttribute('aria-keyshortcuts')).toBe('S')
    await fireEvent.keyDown(document.body, { key: 'S' })
    expect(adapter.seek).toHaveBeenLastCalledWith(90)
    expect(signale).toHaveBeenCalledTimes(1)
  })

  it('reste absent sans intro connue, et visible quand les commandes se masquent', async () => {
    const sans = await ouvert(null, 45)
    expect(bouton(sans.vue)).toBeNull()
    sans.vue.unmount()
    const { vue, handlers } = await ouvert({ debut: 30, fin: 90 }, 45)
    handlers().onState('playing')
    await vi.advanceTimersByTimeAsync(IDLE_MS + 600)
    expect(vue.container.querySelector('.film-player')!.classList.contains('idle')).toBe(true)
    expect(bouton(vue)).toBeTruthy()
  })
})

describe('FilmPlayer : épisode suivant', () => {
  afterEach(() => { vi.useRealTimers(); localStorage.clear(); vi.restoreAllMocks(); location.hash = '' })

  const episode = (n: number, over: Partial<FilmDetail> = {}): FilmDetail => ({ ...detail, id: `e${n}`, kind: 'episode', title: `Épisode ${n}`, season: 1, episode: n, series_id: 's', series_title: 'Les Veilleurs', ...over })
  const saison = (n: number) => ({ id: `s·${n}`, kind: 'season' as const, title: `Saison ${n}`, year: null, overview: null, duration: 0, genres: [], season: n, episode: null, series_id: 's', series_title: null, poster: false, backdrop: false, progress: null, available: true })
  async function ouvert(episodes: Record<string, FilmDetail[]>, t: number) {
    vi.useFakeTimers()
    const position = { t }
    const m = monter(position)
    vi.spyOn(filmsApi, 'item').mockImplementation(async (id) => Object.values(episodes).flat().find((e) => e.id === id)!)
    vi.spyOn(filmsApi, 'seasons').mockResolvedValue({ items: Object.keys(episodes).map((k) => saison(Number(k))) })
    vi.spyOn(filmsApi, 'episodes').mockImplementation(async (sid) => { const items = episodes[sid.split('·')[1]]; return { items, total: items.length } })
    await m.vue.rerender({ id: 'e1' })
    await vi.waitFor(() => expect(m.adapter.load).toHaveBeenCalledWith('e1', 0, true))
    const avancer = async (x: number) => { position.t = x; await vi.advanceTimersByTimeAsync(600) }
    await avancer(t)
    return { ...m, avancer }
  }
  const bouton = (vue: ReturnType<typeof monter>['vue']) => vue.queryByRole('button', { name: 'Épisode suivant' })

  it('apparaît dans les trente dernières secondes et ouvre l’épisode suivant dans le même lecteur', async () => {
    const { vue, adapter, avancer } = await ouvert({ 1: [episode(1), episode(2)] }, 100)
    expect(bouton(vue)).toBeNull()
    await avancer(2975)
    await fireEvent.click(bouton(vue)!)
    expect(location.hash).toBe('#/films/watch/e2')
    await vue.rerender({ id: 'e2' })
    await vi.waitFor(() => expect(adapter.load).toHaveBeenLastCalledWith('e2', 0, true))
    expect(adapter.destroy).not.toHaveBeenCalled()
    await avancer(2975)
    expect(bouton(vue)).toBeNull()
  })

  it('reste proposé une fois l’épisode terminé, et passe à la saison suivante après le dernier épisode', async () => {
    const { vue, handlers, avancer } = await ouvert({ 1: [episode(1)], 2: [episode(3, { season: 2, available: false }), episode(4, { season: 2 })] }, 100)
    handlers().onState('ended')
    await avancer(100)
    await fireEvent.click(bouton(vue)!)
    expect(location.hash).toBe('#/films/watch/e4')
  })

  it('n’existe ni pour un film ni pour le dernier épisode', async () => {
    vi.useFakeTimers()
    const position = { t: 2990 }
    const m = monter(position)
    await vi.waitFor(() => expect(m.adapter.load).toHaveBeenCalled())
    await vi.advanceTimersByTimeAsync(600)
    expect(bouton(m.vue)).toBeNull()
    m.vue.unmount()
    const dernier = await ouvert({ 1: [episode(1)] }, 2990)
    expect(bouton(dernier.vue)).toBeNull()
  })
})
