import { describe, expect, it } from 'vitest'
import { applyOp, chatText, formatClock, ranking, remainingMs, stale, statusOf, type DrawPlayer, type DrawState, type Trace } from './draw'

const p = (id: string, points: number, o: Partial<DrawPlayer> = {}): DrawPlayer => ({ id, pseudo: id, points, turn_points: 0, connected: true, spectator: false, found: false, ...o })

describe('draw', () => {
  it('ajoute des points à un trait, annule et efface', () => {
    const strokes: Trace[] = []
    const op = { op: 'stroke' as const, id: 'a', tool: 'pen' as const, color: '#000000', size: 4, pts: [[0, 0]] as [number, number][] }
    expect(applyOp(strokes, op)).toBe(true)
    applyOp(strokes, { ...op, pts: [[1, 1]] })
    expect(strokes[0].pts).toHaveLength(2)
    applyOp(strokes, { ...op, id: 'b' })
    expect(applyOp(strokes, { op: 'undo' })).toBe(false)
    expect(strokes).toHaveLength(1)
    applyOp(strokes, { op: 'clear' })
    expect(strokes).toHaveLength(0)
  })
  it('calcule le temps restant et l’affiche', () => {
    expect(remainingMs(10_000, 500, 9_000)).toBe(500)
    expect(remainingMs(10_000, 0, 20_000)).toBe(0)
    expect(remainingMs(null, 0, 0)).toBe(0)
    expect(formatClock(47_000)).toBe('0:47')
    expect(formatClock(80_000)).toBe('1:20')
    expect(stale(3, 2)).toBe(true)
    expect(stale(3, 3)).toBe(false)
    expect(stale(undefined, 0)).toBe(false)
  })
  it('classe les joueurs et décrit leur statut en texte', () => {
    expect(ranking([p('a', 10), p('b', 30), p('c', 0, { spectator: true })]).map((x) => x.id)).toEqual(['b', 'a'])
    const s = { phase: 'drawing', drawer: 'a' } as DrawState
    expect(statusOf(p('a', 0), s)).toBe('dessine')
    expect(statusOf(p('b', 0, { found: true }), s)).toBe('trouvé')
    expect(statusOf(p('c', 0, { connected: false }), s)).toBe('absent')
    expect(statusOf(p('d', 0, { spectator: true }), s)).toBe('spectateur')
  })
  it('met en forme les lignes du chat', () => {
    const l = { from: 'b', pseudo: 'Bob', text: 'chien', at: 0 }
    expect(chatText({ ...l, kind: 'guess' }, 'me')).toBe('Bob : chien')
    expect(chatText({ ...l, kind: 'found' }, 'me')).toBe('Bob a trouvé !')
    expect(chatText({ ...l, kind: 'found', from: 'me' }, 'me')).toBe('Tu as trouvé !')
    expect(chatText({ ...l, kind: 'close' }, 'me')).toBe('Tu es proche !')
  })
})
