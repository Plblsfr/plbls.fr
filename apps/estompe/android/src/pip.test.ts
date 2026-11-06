import { describe, it, expect, vi } from 'vitest'
import { pipKind, pipRun, pipState } from './pip'

const none = { watch: false, music: false, solo: false }

describe('pipKind', () => {
  it('choisit Regarder ensemble, puis Music2gether, puis l\'écoute seul', () => {
    expect(pipKind({ watch: true, music: true, solo: true })).toBe('watch')
    expect(pipKind({ ...none, music: true, solo: true })).toBe('music')
    expect(pipKind({ ...none, solo: true })).toBe('solo')
    expect(pipKind(none)).toBeNull()
  })
})

describe('pipState', () => {
  it('est actif seulement avec un lecteur et le réglage activé', () => {
    expect(pipState(true, 'watch', true)).toEqual({ active: true, square: false, playing: true })
    expect(pipState(true, 'music', false)).toEqual({ active: true, square: true, playing: false })
    expect(pipState(false, 'watch', true).active).toBe(false)
    expect(pipState(true, null, false).active).toBe(false)
  })
})

describe('pipRun', () => {
  const actions = () => ({ toggle: vi.fn(), next: vi.fn(), close: vi.fn(), pause: vi.fn() })
  it('relaie les commandes de la fenêtre', () => {
    const a = actions()
    pipRun('toggle', a); pipRun('next', a); pipRun('close', a)
    expect([a.toggle, a.next, a.close].map((f) => f.mock.calls.length)).toEqual([1, 1, 1])
    expect(a.pause).not.toHaveBeenCalled()
  })
  it('met en pause quand la fenêtre est indisponible', () => {
    const a = actions()
    expect(pipRun('unavailable', a)).toBe(true)
    expect(a.pause).toHaveBeenCalledOnce()
    expect(pipRun('enter', a)).toBe(false)
  })
})
