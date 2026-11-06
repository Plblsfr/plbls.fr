import { describe, it, expect, vi } from 'vitest'
import { bgKind, bgPayload, bgRun, stallCheck, STALL_MS } from './bgaudio'

describe('bgKind', () => {
  it('choisit Regarder ensemble, puis Music2gether, puis l\'écoute seul', () => {
    expect(bgKind({ watch: true, music: true, solo: true })).toBe('watch')
    expect(bgKind({ watch: false, music: true, solo: true })).toBe('music')
    expect(bgKind({ watch: false, music: false, solo: true })).toBe('solo')
    expect(bgKind({ watch: false, music: false, solo: false })).toBeNull()
  })
})

describe('bgPayload', () => {
  const s = { title: 'T', artist: 'A', cover: '', playing: true, stalled: false }
  it('n\'envoie rien si l\'option est désactivée ou sans lecteur', () => {
    expect(bgPayload(false, 'solo', s)).toBeNull()
    expect(bgPayload(true, null, s)).toBeNull()
    expect(bgPayload(true, 'music', s)).toBe(s)
  })
})

describe('stallCheck', () => {
  it('signale l\'arrêt quand la position ne bouge plus assez longtemps', () => {
    let r = stallCheck(null, 10, 0, true)
    expect(r.stalled).toBe(false)
    r = stallCheck(r.sample, 10, STALL_MS - 1, true)
    expect(r.stalled).toBe(false)
    r = stallCheck(r.sample, 10, STALL_MS, true)
    expect(r.stalled).toBe(true)
  })
  it('repart quand la position avance ou que la lecture n\'est plus attendue', () => {
    const r = stallCheck({ pos: 10, at: 0 }, 11, STALL_MS * 2, true)
    expect(r).toEqual({ sample: { pos: 11, at: STALL_MS * 2 }, stalled: false })
    expect(stallCheck({ pos: 10, at: 0 }, 10, STALL_MS * 2, false)).toEqual({ sample: null, stalled: false })
  })
})

describe('bgRun', () => {
  it('relaie les boutons de la notification', () => {
    const a = { toggle: vi.fn(), next: vi.fn() }
    bgRun('toggle', a); bgRun('next', a); bgRun('autre', a)
    expect([a.toggle.mock.calls.length, a.next.mock.calls.length]).toEqual([1, 1])
  })
})
