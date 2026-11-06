import { describe, it, expect } from 'vitest'
import { redact, filterEntries, parseLines, debug } from './debuglog.svelte'
import { prefs } from './prefs.svelte'
import { clampScreen, stepDown, screenBitrate } from '../blocks/vocal/voice.svelte'

describe('partage d\'écran', () => {
  it('ramène le choix aux plafonds du serveur', () => {
    expect(clampScreen('2160p', 120, { fps: 60, height: 1440 })).toEqual({ res: '1440p', fps: 60 })
    expect(clampScreen('1080p', 120, { fps: 30, height: 2160 })).toEqual({ res: '1080p', fps: 30 })
    expect(clampScreen('2160p', 120, { fps: 120, height: 2160 })).toEqual({ res: '2160p', fps: 120 })
  })
  it('descend d\'abord les images/s, puis la résolution, puis s\'arrête', () => {
    expect(stepDown({ res: '2160p', fps: 120 })).toEqual({ res: '2160p', fps: 60 })
    expect(stepDown({ res: '2160p', fps: 60 })).toEqual({ res: '2160p', fps: 30 })
    expect(stepDown({ res: '2160p', fps: 30 })).toEqual({ res: '1440p', fps: 30 })
    expect(stepDown({ res: '720p', fps: 30 })).toBeNull()
  })
  it('le débit croît avec les images/s', () => {
    expect(screenBitrate('1080p', 60)).toBeGreaterThan(screenBitrate('1080p', 30))
    expect(screenBitrate('1080p', 120)).toBeGreaterThan(screenBitrate('1080p', 60))
  })
})

describe('mode debug (EST-48)', () => {
  it('masque les secrets', () => {
    expect(redact('Authorization: Bearer abc.def')).not.toContain('abc')
    expect(redact('GET /x?token=secret123&a=1')).toBe('GET /x?token=[masqué]&a=1')
    expect(redact('jeton eyJhbGciOi.eyJzdWIiOi.sig fin')).toBe('jeton [jeton] fin')
    expect(redact('password: "hunter2"')).not.toContain('hunter2')
  })
  it('filtre par niveau et par texte', () => {
    const l = [{ t: 1, l: 'info' as const, s: 'vocal', m: 'connecté' }, { t: 2, l: 'error' as const, s: 'partage', m: 'échec' }]
    expect(filterEntries(l, 'error', '')).toEqual([l[1]])
    expect(filterEntries(l, 'all', 'VOCAL')).toEqual([l[0]])
    expect(filterEntries(l, 'info', 'échec')).toEqual([])
  })
  it('relit les lignes du fichier en ignorant les lignes tronquées', () => {
    expect(parseLines('{"t":1,"l":"info","s":"a","m":"b"}\n{"t":2,"l"\n')).toEqual([{ t: 1, l: 'info', s: 'a', m: 'b' }])
  })
  it("n'écrit rien tant qu'il est désactivé, puis journalise une fois activé et expurgé", async () => {
    debug.log('info', 'test', 'ignoré')
    expect(debug.entries).toHaveLength(0)
    await debug.setEnabled(true)
    debug.log('error', 'test', 'Bearer abc.def')
    expect(prefs.debug).toBe(true)
    expect(debug.entries.at(-1)).toMatchObject({ l: 'error', s: 'test', m: 'Bearer [jeton]' })
    await debug.setEnabled(false)
    const n = debug.entries.length
    debug.log('info', 'test', 'ignoré')
    expect(debug.entries).toHaveLength(n)
    expect(prefs.debug).toBe(false)
  })
})
