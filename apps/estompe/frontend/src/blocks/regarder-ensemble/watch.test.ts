import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { toast } from '../../vendor/plbls-ui/plbls-ui.esm.min.js'
import { notify } from '../../lib/ui'
import { averageColor, clock, coverCrop } from '../musique/music'
import { autoRoom, clockOffset, correction, expectedPosition, lastAction, playerError, roomKind, stale } from './watch'
import { SyncEngine, type Io } from './watchsync'
import { providers, type Adapter, type PState } from './providers'
import { ApiError, filmsApi } from '../../lib/api'
import type { FilmPlayback } from '../films/films'
import type { WatchState } from '../../lib/types'

vi.mock('../../vendor/plbls-ui/plbls-ui.esm.min.js', () => ({ toast: { show: vi.fn(), clear: vi.fn() } }))
vi.mock('../../lib/api', async (original) => ({
  ...(await original<typeof import('../../lib/api')>()),
  filmsApi: { playback: vi.fn(), close: vi.fn(async () => {}), progress: vi.fn(async () => {}) },
}))

const at = '2026-10-28T10:00:00.000Z'
const base = Date.parse(at)
const st = { playing: true, pos: 30, rate: 1, at }

describe('expectedPosition', () => {
  it('avance depuis l\'horodatage du serveur', () => {
    expect(expectedPosition(st, 0, base + 5000)).toBe(35)
    expect(expectedPosition({ ...st, rate: 1.5 }, 0, base + 4000)).toBe(36)
  })
  it('reste fixe en pause', () => {
    expect(expectedPosition({ ...st, playing: false }, 0, base + 60000)).toBe(30)
  })
  it('corrige le décalage d\'horloge', () => {
    // notre horloge a 2 s d\'avance sur celle du serveur : offset = -2000
    const offset = clockOffset(at, base + 2000)
    expect(offset).toBe(-2000)
    expect(expectedPosition(st, offset, base + 5000)).toBe(33)
  })
  it('ne remonte jamais avant le début, même si l\'horloge est en retard', () => {
    expect(expectedPosition({ ...st, pos: 0 }, 0, base - 3000)).toBe(0)
  })
  it('un participant qui arrive en cours de route démarre à la bonne position', () => {
    expect(expectedPosition({ playing: true, pos: 600, rate: 1, at }, 0, base + 90000)).toBe(690)
  })
})

describe('salon cinéma', () => {
  it('lance les films dans un salon cinéma et le reste dans un salon vocal', () => {
    expect(roomKind('cinema')).toBe('cinema')
    expect(roomKind('watch')).toBe('voice')
    expect(roomKind('music')).toBe('voice')
  })
  it('ne choisit seul un salon que si l\'on y est déjà, ou s\'il est le seul et que l\'on n\'est dans aucun autre', () => {
    const rooms = [{ id: 'c1' }]
    expect(autoRoom(rooms, 'c1')).toEqual({ id: 'c1' })
    expect(autoRoom(rooms, null)).toEqual({ id: 'c1' })
    expect(autoRoom(rooms, 'v1')).toBeNull()
    expect(autoRoom([{ id: 'c1' }, { id: 'c2' }], null)).toBeNull()
  })
})

describe('correction', () => {
  it('tolère un petit écart', () => {
    expect(correction(0.1, 50, 1)).toBeNull()
    expect(correction(-0.14, 50, 1)).toBeNull()
  })
  it('rattrape un petit retard en accélérant d\'un cran le temps qu\'il faut', () => {
    expect(correction(-0.5, 50, 1)).toEqual({ rate: 1.25, ms: 2000 })
  })
  it('ralentit quand le lecteur est en avance', () => {
    expect(correction(0.25, 50, 1)).toEqual({ rate: 0.75, ms: 1000 })
  })
  it('saute directement à la bonne position au-delà d\'une seconde', () => {
    expect(correction(-1, 50, 1)).toEqual({ seek: 50 })
    expect(correction(7, 50, 1)).toEqual({ seek: 50 })
  })
  it('saute si la vitesse voulue sort des vitesses de YouTube', () => {
    expect(correction(-0.5, 50, 2)).toEqual({ seek: 50 })
    expect(correction(0.5, 50, 0.25)).toEqual({ seek: 50 })
  })
})

describe('messages', () => {
  it('nomme la dernière action', () => {
    expect(lastAction({ act: 'pause', by: 'Alice' })).toBe('Alice a mis en pause')
    expect(lastAction({ act: 'ended', by: 'Alice' })).toBe('')
  })
  it('explique une vidéo non intégrable', () => {
    expect(playerError(150)).toContain("n'autorise pas")
    expect(playerError(100)).toContain('introuvable')
    expect(playerError(999)).toBe('Lecture impossible.')
  })
})

describe('états du serveur', () => {
  it('rejette un état plus ancien que celui déjà appliqué, accepte le même ou un plus récent', () => {
    expect(stale({ rev: 5 }, { rev: 4 })).toBe(true)
    expect(stale({ rev: 5 }, { rev: 5 })).toBe(false)
    expect(stale({ rev: 5 }, { rev: 6 })).toBe(false)
    expect(stale(undefined, { rev: 1 })).toBe(false)
  })
})

describe('moteur de synchronisation', () => {
  const mk = (o: Partial<WatchState> = {}) => ({ rev: 1, provider: 'youtube', video: 'v', playing: true, pos: 10, rate: 1, at, host: 'me', host_only: false, by_id: 'me', by: 'moi', act: 'play', queue: [], ready: {}, repeat: '' as const, history: [], ...o }) as WatchState
  function setup(o: Partial<WatchState> = {}) {
    vi.useFakeTimers()
    vi.setSystemTime(base)
    let state = mk(o)
    const pl = { st: 'playing' as PState, t: 10, r: 1, calls: [] as string[] }
    const p: Adapter = {
      sync: 'full', canRate: true,
      load: (ref) => { pl.calls.push('load ' + ref) },
      play: () => { pl.calls.push('play'); pl.st = 'playing' }, pause: () => { pl.calls.push('pause'); pl.st = 'paused' },
      seek: (s) => { pl.calls.push('seek ' + s); pl.t = s }, time: () => pl.t, rate: () => pl.r, setRate: (r) => { pl.calls.push('rate ' + r); pl.r = r },
      state: () => pl.st, destroy() {},
    }
    const sent: { action: string; position?: number; video: string }[] = []
    let answer: (ok: boolean) => void = () => {}
    const io: Io = {
      server: () => ({ state, offset: 0 }),
      send: (action, b) => { sent.push({ action, ...b }); return new Promise((res, rej) => { answer = (ok) => (ok ? res(undefined) : rej(new Error('refusé'))) }) },
      ended: vi.fn(), ready: vi.fn(), refused: vi.fn(), log() {},
    }
    const e = new SyncEngine(p, io)
    const push = (n: Partial<WatchState>) => { state = { ...state, ...n, at: new Date(Date.now()).toISOString() }; e.receive() }
    e.receive() // charge la vidéo
    e.observe('playing')
    pl.calls.length = 0
    return { e, pl, sent, push, io, answer: (ok: boolean) => answer(ok), state: () => state }
  }

  it('charge la vidéo puis passe à « synchronisé » quand le lecteur répond', () => {
    vi.useFakeTimers()
    const t = setup()
    expect(t.e.phase).toBe('synced')
    vi.useRealTimers()
  })

  it('l\'état reçu est appliqué au lecteur sans jamais produire de commande', () => {
    const t = setup()
    t.push({ rev: 2, playing: false, pos: 42, act: 'pause' })
    expect(t.pl.calls).toEqual(['pause', 'seek 42'])
    t.push({ rev: 3, playing: true, pos: 42, act: 'play' })
    expect(t.pl.calls).toContain('play')
    expect(t.sent).toEqual([])
    vi.useRealTimers()
  })

  it('une commande s\'applique tout de suite puis attend la confirmation du serveur', () => {
    const t = setup()
    t.e.command({ action: 'pause', position: 12 })
    expect(t.pl.calls).toEqual(['pause'])
    expect(t.sent).toEqual([{ action: 'pause', position: 12, video: 'v' }])
    expect(t.e.phase).toBe('pending')
    t.push({ rev: 2, playing: false, pos: 12, act: 'pause' })
    expect(t.e.phase).toBe('synced')
    vi.useRealTimers()
  })

  it('un état plus ancien que la dernière commande locale n\'est pas appliqué (pas de retour en arrière)', () => {
    const t = setup()
    t.e.command({ action: 'pause', position: 12 })
    t.pl.calls.length = 0
    t.push({ rev: 1, playing: true, pos: 10 }) // état reçu en retard
    expect(t.pl.calls).toEqual([])
    expect(t.e.phase).toBe('pending')
    vi.useRealTimers()
  })

  it('des commandes enchaînées vite sont envoyées une par une, seule la dernière de chaque réglage est gardée', () => {
    const t = setup()
    t.e.command({ action: 'pause', position: 12 })
    t.e.command({ action: 'seek', position: 20 })
    t.e.command({ action: 'seek', position: 30 })
    t.e.command({ action: 'play', position: 30 })
    expect(t.sent.map((c) => c.action)).toEqual(['pause'])
    t.push({ rev: 2, playing: false, pos: 12, act: 'pause' })
    expect(t.sent.map((c) => c.action)).toEqual(['pause', 'play']) // play (avec position) remplace les déplacements
    expect(t.sent[1].position).toBe(30)
    vi.useRealTimers()
  })

  it('une commande refusée ramène le lecteur à l\'état du serveur', async () => {
    const t = setup()
    t.e.command({ action: 'pause', position: 12 })
    t.pl.calls.length = 0
    t.answer(false)
    await vi.advanceTimersByTimeAsync(0)
    expect(t.io.refused).toHaveBeenCalled()
    expect(t.pl.calls).toContain('play')
    expect(t.e.phase).toBe('synced')
    vi.useRealTimers()
  })

  it('une commande perdue est abandonnée au bout de quelques secondes', async () => {
    const t = setup()
    t.e.command({ action: 'pause', position: 12 })
    await vi.advanceTimersByTimeAsync(3100)
    expect(t.io.refused).toHaveBeenCalled()
    expect(t.e.phase).toBe('synced')
    vi.useRealTimers()
  })

  it('le recalage ne relance jamais la lecture, ne touche pas la pause et attend la commande en cours', () => {
    const t = setup()
    t.pl.st = 'paused'
    t.e.tick()
    expect(t.pl.calls).toEqual([])
    t.pl.st = 'playing'
    t.pl.t = 3 // très en retard
    t.e.command({ action: 'rate', rate: 1.5 })
    t.pl.calls.length = 0
    t.e.tick()
    expect(t.pl.calls).toEqual([])
    vi.useRealTimers()
  })

  it('recale un grand écart par un saut, un petit par un cran de vitesse, une correction à la fois', () => {
    const t = setup()
    t.pl.t = 3
    t.e.tick()
    expect(t.pl.calls).toEqual(['seek 10'])
    t.pl.calls.length = 0
    t.pl.t = 9.5
    t.e.tick()
    t.e.tick()
    expect(t.pl.calls).toEqual(['rate 1.25'])
    vi.useRealTimers()
  })

  it('les évènements du lecteur n\'émettent aucune commande : la fin de vidéo seule est signalée', () => {
    const t = setup()
    t.pl.st = 'paused'
    t.e.observe('paused')
    t.e.observe('buffering')
    t.e.observe('playing')
    expect(t.sent).toEqual([])
    t.e.observe('ended')
    expect(t.io.ended).toHaveBeenCalledWith(1)
    vi.useRealTimers()
  })

  it('un état identique (indicateurs « prêt ») ne relance rien', () => {
    const t = setup()
    t.pl.calls.length = 0
    t.e.receive()
    t.e.receive()
    expect(t.pl.calls).toEqual([])
    vi.useRealTimers()
  })

  it('un changement de vidéo charge la nouvelle et abandonne les commandes en attente', () => {
    const t = setup()
    t.e.command({ action: 'pause', position: 12 })
    t.push({ rev: 2, video: 'w', playing: true, pos: 0, act: 'video' })
    expect(t.pl.calls).toContain('load w')
    expect(t.e.phase).toBe('loading')
    vi.useRealTimers()
  })

  it('une tempête d\'évènements suspend la session au lieu de figer l\'application, puis reprend', () => {
    const t = setup()
    for (let i = 2; i < 40; i++) t.push({ rev: i, pos: i })
    expect(t.e.suspended).toBe(true)
    t.pl.calls.length = 0
    t.push({ rev: 99 })
    expect(t.pl.calls).toEqual([])
    vi.advanceTimersByTime(5100)
    expect(t.e.suspended).toBe(false)
    vi.useRealTimers()
  })

  it('10 participants qui enchaînent des changements restent cohérents : le dernier état du serveur l\'emporte', () => {
    const t = setup()
    for (let i = 2; i < 12; i++) { t.push({ rev: i, playing: i % 2 === 0, pos: i, act: i % 2 ? 'pause' : 'play' }); vi.advanceTimersByTime(120) }
    expect(t.pl.st).toBe('paused') // dernier état : rév 11, en pause
    expect(t.sent).toEqual([])
    vi.useRealTimers()
  })

  it('lecture automatique refusée : signalée, sans relancer le lecteur', () => {
    const t = setup()
    t.pl.st = 'paused'
    for (let i = 0; i < 8; i++) { t.e.tick(); vi.advanceTimersByTime(500) }
    expect(t.e.stalled).toBe(true)
    expect(t.pl.calls).toEqual([])
    vi.useRealTimers()
  })
})

describe('Music2gether', () => {
  it('formate la durée d\'un morceau', () => {
    expect(clock(205)).toBe('3:25')
    expect(clock(3723)).toBe('1:02:03')
    expect(clock(9)).toBe('0:09')
    expect(clock(0)).toBe('')
  })
  it('recadre la miniature 4:3 sur la pochette carrée, sans les bandes noires', () => {
    expect(coverCrop(480, 360)).toEqual({ x: 105, y: 45, side: 270 })
    expect(coverCrop(100, 360)).toEqual({ x: 0, y: 130, side: 100 })
  })
  it('calcule la couleur moyenne d\'une pochette en ignorant les pixels transparents', () => {
    expect(averageColor([255, 0, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0])).toBe('rgb(128, 0, 128)')
    expect(averageColor([1, 2, 3, 0])).toBe('')
  })
})

describe('notify', () => {
  afterEach(() => vi.useRealTimers())

  it('regroupe les notifications identiques avec un compteur et en limite trois à la fois', () => {
    vi.useFakeTimers()
    const show = vi.mocked(toast.show)
    show.mockClear()
    for (let i = 0; i < 12; i++) notify.fail('Trop de demandes')
    expect(show).toHaveBeenLastCalledWith('Trop de demandes (×12)', 'erreur', 5000)
    for (const m of ['a', 'b', 'c']) notify.fail(m)
    expect(show.mock.calls.slice(-3).map((c) => c[0])).toEqual(['Trop de demandes (×12)', 'a', 'b'])
    vi.advanceTimersByTime(6000)
    expect(show).toHaveBeenLastCalledWith('c', 'erreur', 5000)
  })
})

describe('lecteur de film', () => {
  const stream = { stream_id: 'f1', mode: 'direct', url: '/films/stream/t/direct', duration: 3000, audio: [{ n: 0, label: 'Français', language: 'fra' }, { n: 1, label: 'Anglais', language: 'eng' }], subtitles: [{ n: 0, label: 'Français', language: 'fra', url: '/films/stream/t/sub/0' }] }
  const handlers = () => ({ onState: vi.fn(), onError: vi.fn(), onTracks: vi.fn() })
  const settled = () => vi.waitFor(() => expect(vi.mocked(filmsApi.playback)).toHaveBeenCalled())
  const opened = async (n: number) => vi.waitFor(() => expect(vi.mocked(filmsApi.playback)).toHaveBeenCalledTimes(n))

  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(filmsApi.playback).mockResolvedValue(stream as FilmPlayback)
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {})
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('probably')
  })

  it('demande un flux signé, annonce les pistes et ferme le flux en quittant', async () => {
    const h = handlers()
    const el = document.createElement('div')
    const a = await providers.film(el, h)
    a.load('e1', 0, true)
    await settled()
    expect(filmsApi.playback).toHaveBeenCalledWith('e1', { direct: true, audio: 0 })
    await vi.waitFor(() => expect(h.onTracks).toHaveBeenCalled())
    expect(el.querySelector('video')?.src).toContain('/films/stream/t/direct')
    expect(el.querySelector('video')?.crossOrigin).toBe('anonymous')
    expect(el.querySelector('track')?.getAttribute('src')).toContain('/films/stream/t/sub/0')
    a.destroy()
    expect(filmsApi.close).toHaveBeenCalledWith('f1')
  })

  it('une piste audio choisie rouvre le flux, propre au spectateur', async () => {
    const a = await providers.film(document.createElement('div'), handlers())
    a.load('e1', 0, true)
    await settled()
    await vi.waitFor(() => expect(HTMLMediaElement.prototype.load).toHaveBeenCalled())
    a.setAudio?.(1)
    await opened(2)
    expect(filmsApi.playback).toHaveBeenLastCalledWith('e1', { direct: true, audio: 1 })
    expect(filmsApi.close).toHaveBeenCalledWith('f1')
    a.destroy()
  })

  it('une panne de lecture rouvre une fois en transcodage à la même position, puis l\'affiche', async () => {
    const h = handlers()
    const el = document.createElement('div')
    const a = await providers.film(el, h)
    a.load('e1', 120, true)
    await settled()
    await vi.waitFor(() => expect(HTMLMediaElement.prototype.load).toHaveBeenCalled())
    const video = el.querySelector('video')!
    video.dispatchEvent(new Event('error'))
    await opened(2)
    expect(filmsApi.playback).toHaveBeenLastCalledWith('e1', { direct: false, audio: 0 })
    expect(h.onError).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(HTMLMediaElement.prototype.load).toHaveBeenCalledTimes(2))
    video.dispatchEvent(new Event('error'))
    expect(h.onError).toHaveBeenCalledTimes(1)
    a.destroy()
  })

  it('enregistre la progression du spectateur à la pause et en quittant', async () => {
    const el = document.createElement('div')
    const a = await providers.film(el, handlers())
    a.load('e1', 120, true)
    await settled()
    await vi.waitFor(() => expect(HTMLMediaElement.prototype.load).toHaveBeenCalled())
    el.querySelector('video')!.dispatchEvent(new Event('pause'))
    expect(filmsApi.progress).toHaveBeenLastCalledWith('e1', 120, 3000)
    a.destroy()
    expect(filmsApi.progress).toHaveBeenCalledTimes(2)
  })

  it('un refus du serveur est montré au spectateur', async () => {
    vi.mocked(filmsApi.playback).mockRejectedValue(new ApiError(503, 'serveur_charge', 'le serveur est trop chargé'))
    const h = handlers()
    const a = await providers.film(document.createElement('div'), h)
    a.load('e1', 0, true)
    await vi.waitFor(() => expect(h.onError).toHaveBeenCalledWith('le serveur est trop chargé'))
    a.destroy()
  })

  it('le lancement d\'un autre appareil sans codec de base passe en transcodage', async () => {
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('')
    const a = await providers.film(document.createElement('div'), handlers())
    a.load('e1', 0, true)
    await settled()
    expect(filmsApi.playback).toHaveBeenCalledWith('e1', { direct: false, audio: 0 })
    a.destroy()
  })
})
