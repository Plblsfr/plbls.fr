import { afterEach, describe, expect, it, vi } from 'vitest'
import { lastAction, waitingText } from './watch'
import { SyncEngine, type Io } from './watchsync'
import type { Adapter, PState } from './providers'
import type { WatchState } from '../../lib/types'

const at = '2026-10-28T10:00:00.000Z'
const mk = (o: Partial<WatchState> = {}) => ({ rev: 1, provider: 'film', video: 'v', playing: true, pos: 10, rate: 1, at, host: 'me', host_only: true, by_id: 'me', by: 'moi', act: 'play', queue: [], ready: {}, repeat: '' as const, history: [], ...o }) as WatchState

function setup(wait = true) {
  vi.useFakeTimers()
  vi.setSystemTime(Date.parse(at))
  let state = mk()
  const pl = { st: 'unstarted' as PState, ready: false, calls: [] as string[] }
  const p: Adapter = {
    sync: 'full', canRate: true,
    load: () => { pl.calls.push('load') }, play: () => { pl.calls.push('play') }, pause: () => { pl.calls.push('pause'); pl.st = 'paused' },
    seek: (s) => { pl.calls.push('seek ' + s) }, time: () => 10, rate: () => 1, setRate() {},
    state: () => pl.st, ready: () => pl.ready, destroy() {},
  }
  const asked = vi.fn<() => void>()
  const io = { server: () => ({ state, offset: 0 }), send: () => Promise.resolve(), ended: vi.fn(), ready: vi.fn<(r: boolean) => void>(), refused: vi.fn(), log() {}, wait: asked } satisfies Io
  const e = new SyncEngine(p, wait ? io : { ...io, wait: undefined })
  const push = (n: Partial<WatchState>) => { state = { ...state, ...n, at: new Date(Date.now()).toISOString() }; e.receive() }
  const pass = (ms: number) => { for (let i = 0; i < ms; i += 500) { vi.advanceTimersByTime(500); e.tick() } }
  e.receive()
  return { e, pl, io, push, pass }
}

describe('salon cinéma : attente d’un participant qui charge', () => {
  afterEach(() => vi.useRealTimers())

  it('demande la pause commune quand le chargement dure, une seule fois', () => {
    const t = setup()
    t.pass(500)
    expect(t.io.wait).not.toHaveBeenCalled()
    t.pass(1500)
    expect(t.io.wait).toHaveBeenCalledTimes(1)
    t.pass(5000)
    expect(t.io.wait).toHaveBeenCalledTimes(1)
    expect(t.e.loading).toBe(true)
  })

  it('ne demande rien pour une courte attente de données', () => {
    const t = setup()
    t.pl.st = 'playing'
    t.e.observe('playing')
    t.pl.st = 'buffering'
    t.e.observe('buffering')
    t.pass(500)
    t.pl.st = 'playing'
    t.e.observe('playing')
    t.pass(3000)
    expect(t.io.wait).not.toHaveBeenCalled()
    expect(t.io.ready).toHaveBeenLastCalledWith(true)
  })

  it('une fois mis en pause, ne se dit prêt que lorsque assez de vidéo est chargée', () => {
    const t = setup()
    t.pl.st = 'buffering'
    t.e.observe('buffering')
    t.pass(1500)
    expect(t.io.wait).toHaveBeenCalledTimes(1)
    t.push({ rev: 2, playing: false, act: 'wait', waiting: ['me'] })
    t.pl.st = 'paused'
    t.e.observe('paused')
    t.pass(2000)
    expect(t.io.ready).not.toHaveBeenCalledWith(true)
    t.pl.ready = true
    t.pass(500)
    expect(t.io.ready).toHaveBeenLastCalledWith(true)
  })

  it('applique la pause commune arrivée pendant son propre chargement', () => {
    const t = setup()
    t.push({ rev: 2, playing: false, act: 'wait', waiting: ['bob'] })
    t.pl.ready = true
    t.pl.st = 'playing'
    t.e.observe('playing')
    expect(t.pl.calls).toContain('pause')
  })

  it('rejoint l’attente en cours si son lecteur charge à son tour', () => {
    const t = setup()
    t.pl.ready = true
    t.pl.st = 'paused'
    t.e.observe('paused')
    t.push({ rev: 2, playing: false, act: 'wait', waiting: ['bob'] })
    t.pl.st = 'buffering'
    t.e.observe('buffering')
    t.pass(1500)
    expect(t.io.wait).toHaveBeenCalledTimes(1)
  })

  it('ne demande rien en pause voulue ni hors du salon cinéma', () => {
    const t = setup()
    t.push({ rev: 2, playing: false, act: 'pause' })
    t.pass(3000)
    expect(t.io.wait).not.toHaveBeenCalled()
    const u = setup(false)
    u.pass(3000)
    expect(u.io.wait).not.toHaveBeenCalled()
  })

  it('nomme les personnes attendues', () => {
    const people = [{ user_id: 'a', pseudo: 'Alice' }, { user_id: 'b', pseudo: 'Bob' }]
    expect(waitingText(undefined, 'a', people)).toBe('')
    expect(waitingText([], 'a', people)).toBe('')
    expect(waitingText(['b'], 'a', people)).toBe('Lecture en pause : Bob charge la vidéo. Elle reprendra toute seule.')
    expect(waitingText(['a', 'b'], 'a', people)).toBe('Lecture en pause : votre appareil et Bob chargent la vidéo. Elle reprendra toute seule.')
    expect(waitingText(['z'], 'a', people)).toBe('Lecture en pause : quelqu’un charge la vidéo. Elle reprendra toute seule.')
    expect(lastAction({ act: 'wait', by: 'Bob' })).toBe('Lecture en pause : Bob charge la vidéo')
    expect(lastAction({ act: 'resume', by: 'Bob' })).toBe('')
  })
})
