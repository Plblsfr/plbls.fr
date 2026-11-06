import { describe, expect, it, vi } from 'vitest'
import { buildItems, collectWatchers, gridDims, shouldSubscribe } from './stage'
import type { VideoTile } from './voice.svelte'

const tile = (sid: string, userId: string, screen = false) => ({ sid, userId, name: userId, screen, track: {} as never }) as VideoTile
const people = [{ user_id: 'a', pseudo: 'alice' }, { user_id: 'b', pseudo: 'bob' }]

describe('buildItems', () => {
  it('donne une tuile par participant, avatar si pas de caméra', () => {
    const items = buildItems(people, [tile('t1', 'b')])
    expect(items.map((i) => [i.userId, !!i.tile])).toEqual([['a', false], ['b', true]])
  })
  it('ajoute une tuile pour chaque partage d\'écran', () => {
    const items = buildItems(people, [tile('t1', 'a'), tile('t2', 'a', true)])
    expect(items.map((i) => i.key)).toEqual(['t1', 't2', 'u:b'])
  })
  it('affiche un flux dont le participant n\'est pas encore listé', () => {
    expect(buildItems([], [tile('t9', 'z')]).map((i) => i.userId)).toEqual(['z'])
  })
})

describe('gridDims', () => {
  it.each([[1, 1, 1], [2, 2, 1], [3, 2, 2], [4, 2, 2], [5, 3, 2], [9, 3, 3], [10, 4, 3]])('%i tuiles : %i colonnes, %i lignes', (n, cols, rows) => {
    expect(gridDims(n)).toEqual({ cols, rows })
  })
  it('empile deux tuiles sur un écran étroit', () => {
    expect(gridDims(2, true)).toEqual({ cols: 1, rows: 2 })
    expect(gridDims(3, true)).toEqual({ cols: 2, rows: 2 })
  })
})

describe('collectWatchers', () => {
  it('regroupe les spectateurs par flux', () => {
    const w = collectWatchers([{ id: 'a', name: 'alice', watching: 't1,t2' }, { id: 'b', name: 'bob', watching: 't1' }, { id: 'c', name: 'carl', watching: '' }, { id: 'd', name: 'dan' }])
    expect(w).toEqual({ t1: [{ id: 'a', name: 'alice' }, { id: 'b', name: 'bob' }], t2: [{ id: 'a', name: 'alice' }] })
  })
})

describe('shouldSubscribe', () => {
  it('reçoit toujours micro et caméra', () => {
    expect(shouldSubscribe('microphone', undefined, false)).toBe(true)
    expect(shouldSubscribe('camera', false, false)).toBe(true)
  })
  it('ne reçoit pas un écran non regardé par défaut', () => {
    expect(shouldSubscribe('screen_share', undefined, false)).toBe(false)
    expect(shouldSubscribe('screen_share_audio', undefined, false)).toBe(false)
  })
  it('reçoit l\'écran après « Regarder », et plus après « Arrêter »', () => {
    expect(shouldSubscribe('screen_share', true, false)).toBe(true)
    expect(shouldSubscribe('screen_share', false, true)).toBe(false)
  })
  it('respecte le réglage automatique', () => {
    expect(shouldSubscribe('screen_share_audio', undefined, true)).toBe(true)
  })
  it('ne rejoue jamais le son de son propre partage, mais garde l\'aperçu vidéo', () => {
    expect(shouldSubscribe('screen_share_audio', true, true, true)).toBe(false)
    expect(shouldSubscribe('screen_share', true, true, true)).toBe(true)
  })
})

describe('buildItems sans flux reçu', () => {
  it('garde une tuile d\'écran non regardée (sans piste)', () => {
    const t = { sid: 's', userId: 'a', name: 'a', screen: true } as VideoTile
    expect(buildItems(people, [t]).map((i) => [i.key, !!i.tile?.track])).toEqual([['u:a', false], ['s', false], ['u:b', false]])
  })
})

describe('réglage watchAuto (EST-81)', () => {
  const charge = async (stocke?: object) => {
    localStorage.clear()
    if (stocke) localStorage.setItem('estompe.prefs', JSON.stringify(stocke))
    vi.resetModules()
    return (await import('../../lib/prefs.svelte')).prefs
  }
  it('regarde par défaut, sans réglage enregistré', async () => {
    expect((await charge()).watchAuto).toBe(true)
  })
  it('applique le nouveau défaut si l\'option n\'a jamais été réglée explicitement', async () => {
    expect((await charge({ watchAuto: false, theme: 'dark' })).watchAuto).toBe(true)
  })
  it('conserve un choix explicite « ne pas regarder »', async () => {
    expect((await charge({ watchAuto: false, watchAutoSet: true })).watchAuto).toBe(false)
  })
})
