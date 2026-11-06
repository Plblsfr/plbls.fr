import { describe, expect, it } from 'vitest'
import { audioMix, clampCrop, formatTimecode, frameDuration, parseTimecode, planLabel, resizeCrop, snapToKeyframe, audioTracks, clipAround, compressedSize, cropForRatio, destinations, estimateSize, filterDestinations, formatDuration, formatSize, isTransient, markerCount, markerName, needsCompression, sendErrorMessage, targetKbps, trackVolume, visible, withMarker, withRetry, type Replay } from './replays'
import { gameOfName } from '../../frontend/src/lib/format'

const r = (name: string, game: string, modified: number, duration: number, size: number): Replay => ({ name, game, modified, duration, size, width: 1920, height: 1080, fps: 60, markers: [], tracks: [] })
const list = [r('Hades_1.mp4', 'Hades', 1, 30, 300e6), r('Celeste_1.mp4', 'Celeste', 3, 60, 100e6), r('Hades_2.mp4', 'Hades', 2, 10, 200e6)]

describe('replays', () => {
  it('trie, filtre par jeu et cherche par nom', () => {
    expect(visible(list, '', '', 'date').map((x) => x.name)).toEqual(['Celeste_1.mp4', 'Hades_2.mp4', 'Hades_1.mp4'])
    expect(visible(list, '', '', 'size')[0].name).toBe('Hades_1.mp4')
    expect(visible(list, '', '', 'duration')[0].name).toBe('Celeste_1.mp4')
    expect(visible(list, '', 'Hades', 'date')).toHaveLength(2)
    expect(visible(list, 'CELE', '', 'date')).toHaveLength(1)
  })

  it('calcule la zone des ratios prédéfinis, en pixels pairs', () => {
    expect(cropForRatio(null, 1920, 1080)).toBeNull()
    expect(cropForRatio(1, 1920, 1080)).toEqual({ x: 420, y: 0, w: 1080, h: 1080 })
    expect(cropForRatio(9 / 16, 1920, 1080)).toEqual({ x: 656, y: 0, w: 606, h: 1080 })
    expect(cropForRatio(16 / 9, 1920, 1080)).toEqual({ x: 0, y: 0, w: 1920, h: 1080 })
  })

  it('estime la taille exportée', () => {
    const a = list[0]
    expect(estimateSize(a, 0, 15, null, 'original')).toBe(150e6)
    expect(estimateSize(a, 0, 30, null, '720')).toBeCloseTo(300e6 * (720 / 1080) ** 2)
    expect(estimateSize(a, 0, 30, { x: 0, y: 0, w: 100, h: 100 }, 'original')).toBe(300e6 * 0.25)
  })

  it('formate durée et taille', () => {
    expect(formatDuration(62.4)).toBe('1:02')
    expect(formatDuration(3725)).toBe('1:02:05')
    expect(formatSize(1.5e9)).toBe('1.5 Go')
    expect(formatSize(42e6)).toBe('42 Mo')
  })
})

describe('marqueurs', () => {
  const m = (t: number, name = `Marqueur ${t}`) => ({ t, name })
  it('découpe autour d\'un marqueur, bornée au replay', () => {
    expect(clipAround(30, 10, 5, 60)).toEqual([20, 35])
    expect(clipAround(4, 10, 5, 60)).toEqual([0, 9])
    expect(clipAround(58, 10, 5, 60)).toEqual([48, 60])
    expect(clipAround(30, -3, 5, 60)).toEqual([30, 35])
  })
  it('ajoute, renomme et retire en gardant l\'ordre du temps', () => {
    const a = m(10), b = m(40)
    expect(withMarker([b], a).map((x) => x.t)).toEqual([10, 40])
    expect(withMarker([a, b], a, '  Boss  ')).toEqual([{ t: 10, name: 'Boss' }, b])
    expect(withMarker([a, b], a, '  ')).toEqual([a, b])
    expect(withMarker([a, b], b, null)).toEqual([a])
  })
  it('nomme les marqueurs ajoutés et compte ceux d\'un replay', () => {
    expect(markerName([])).toBe('Marqueur 1')
    expect(markerName([m(1, 'Marqueur 1'), m(2, 'Marqueur 3')])).toBe('Marqueur 4')
    expect(markerCount({ markers: [] })).toBe('')
    expect(markerCount({ markers: [m(1)] })).toBe('1 marqueur')
    expect(markerCount({ markers: [m(1), m(2)] })).toBe('2 marqueurs')
  })
})

describe('envoi d\'un replay', () => {
  const MB = 1024 * 1024
  const err = (status: number, code = '') => Object.assign(new Error('x'), { status, code })

  it('lit le jeu dans le nom du fichier', () => {
    expect(gameOfName('Dead_Cells_2026-10-27_20-15-03.mp4')).toBe('Dead Cells')
    expect(gameOfName('vacances.mp4')).toBe('')
  })

  it('propose la compression au-delà de la limite et dimensionne le débit pour la respecter', () => {
    expect(needsCompression(100 * MB, 100 * MB)).toBe(false)
    expect(needsCompression(100 * MB + 1, 100 * MB)).toBe(true)
    const kbps = targetKbps(100 * MB, 120)
    expect(((kbps + 128) * 1000 * 120) / 8).toBeLessThanOrEqual(100 * MB * 0.9)
    expect(targetKbps(10 * MB, 3600)).toBe(500) // plancher
    const r = { duration: 120, size: 400 * MB }
    expect(compressedSize(r, kbps)).toBeLessThanOrEqual(100 * MB)
    expect(compressedSize({ duration: 10, size: 1 * MB }, 8000)).toBe(1 * MB) // jamais plus gros que l'original
  })

  it('retente les coupures réseau et les pannes, pas les refus', async () => {
    expect([err(0), err(503), err(429, 'too_many_video_uploads')].every(isTransient)).toBe(true)
    expect([err(413), err(415), err(403), err(429, 'slowmode'), new Error('x')].some(isTransient)).toBe(false)
    let calls = 0
    const waits: number[] = []
    const ok = await withRetry(async () => { if (++calls < 3) throw err(0); return 'ok' }, [1, 2, 3], async (ms) => void waits.push(ms))
    expect([ok, calls, waits]).toEqual(['ok', 3, [1, 2]])
    calls = 0
    await expect(withRetry(async () => { calls++; throw err(413) }, [1, 2], async () => {})).rejects.toMatchObject({ status: 413 })
    expect(calls).toBe(1)
    calls = 0
    await expect(withRetry(async () => { calls++; throw err(0) }, [1, 2], async () => {})).rejects.toMatchObject({ status: 0 })
    expect(calls).toBe(3)
  })

  it('liste les destinations où l\'on peut joindre des fichiers, avec recherche', () => {
    const servers = [{ id: 's1', name: 'Les potes' }, { id: 's2', name: 'Boulot' }]
    const channels = {
      s1: [{ id: 'c1', name: 'général', kind: 'text' }, { id: 'c2', name: 'vocal', kind: 'voice' }, { id: 'c3', name: 'annonces', kind: 'text' }],
      s2: [{ id: 'c4', name: 'général', kind: 'text' }],
    }
    const list = destinations(servers, channels, [{ id: 'd1', other: { pseudo: 'Alice' } }], (k) => k !== 'c:c3')
    expect(list.map((d) => d.key)).toEqual(['c:c1', 'c:c4', 'd:d1'])
    expect(filterDestinations(list, 'boulot').map((d) => d.key)).toEqual(['c:c4'])
    expect(filterDestinations(list, 'ali').map((d) => d.key)).toEqual(['d:d1'])
    expect(filterDestinations(list, '  ')).toHaveLength(3)
  })

  it('explique les refus du serveur', () => {
    expect(sendErrorMessage({ status: 413, code: 'file_too_large', message: '100 Mo maximum pour une vidéo' })).toBe('100 Mo maximum pour une vidéo')
    expect(sendErrorMessage({ status: 415, code: 'unsupported_video', message: 'vidéo MP4 attendue' })).toBe('vidéo MP4 attendue')
    expect(sendErrorMessage({ status: 0 })).toBe('Connexion au serveur impossible.')
  })

  it('liste les pistes réglables sans la piste Mix', () => {
    expect(audioTracks({ tracks: [] })).toEqual([])
    expect(audioTracks({ tracks: ['Micro'] })).toEqual([])
    expect(audioTracks({ tracks: ['Mix', 'Jeu'] })).toEqual([])
    expect(audioTracks({ tracks: ['Mix', 'Jeu', 'Vocal', 'Micro'] }).map((t) => [t.index, t.name])).toEqual([[1, 'Jeu'], [2, 'Vocal'], [3, 'Micro']])
  })

  it('compose la demande d\'export des pistes', () => {
    const t = audioTracks({ tracks: ['Mix', 'Jeu', 'Vocal', 'Micro'] })
    expect(audioMix(t, true)).toBeUndefined()
    expect(audioMix([], false)).toBeUndefined()
    t[1].on = false
    expect(audioMix(t, true)).toEqual({ tracks: [{ index: 1, volume: 1 }, { index: 3, volume: 1 }], mixed: true })
    t[0].volume = 0.5
    expect(audioMix(t, false)?.tracks[0]).toEqual({ index: 1, volume: 0.5 })
    expect(audioMix(t.map((x) => ({ ...x, on: false })), false)).toEqual({ tracks: [], mixed: false })
    expect(trackVolume(t[1], 1)).toBe(0)
    expect(trackVolume(t[0], 0.5)).toBe(0.25)
  })

  it('saisit et affiche les timecodes', () => {
    expect(parseTimecode('90')).toBe(90)
    expect(parseTimecode('1:30.5')).toBe(90.5)
    expect(parseTimecode('1:02:03')).toBe(3723)
    expect(parseTimecode('1,5')).toBe(1.5)
    expect(parseTimecode('abc')).toBeNull()
    expect(parseTimecode('1:2:3:4')).toBeNull()
    expect(formatTimecode(90.5)).toBe('1:30.5')
    expect(formatTimecode(3723)).toBe('1:02:03.0')
    expect(parseTimecode(formatTimecode(754.3))).toBeCloseTo(754.3)
  })

  it("s'aimante sur l'image clé la plus proche", () => {
    expect(snapToKeyframe(10.2, [0, 10, 20], 0.5)).toBe(10)
    expect(snapToKeyframe(12, [0, 10, 20], 0.5)).toBe(12)
    expect(snapToKeyframe(5, [], 1)).toBe(5)
    expect(snapToKeyframe(10.3, [10, 10.4], 1)).toBe(10.4)
  })

  it("avance d'une image selon la fréquence de la vidéo", () => {
    expect(frameDuration(30)).toBeCloseTo(1 / 30)
    expect(frameDuration(0)).toBeCloseTo(1 / 60)
  })

  it('garde la zone de recadrage dans l’image, en dimensions paires', () => {
    expect(clampCrop({ x: -5, y: 1001, w: 641, h: 5000 }, 1920, 1080)).toEqual({ x: 0, y: 0, w: 642, h: 1080 })
    expect(clampCrop({ x: 1900, y: 0, w: 100, h: 100 }, 1920, 1080)).toEqual({ x: 1820, y: 0, w: 100, h: 100 })
    expect(clampCrop({ x: 0, y: 0, w: 3, h: 3 }, 1920, 1080)).toMatchObject({ w: 32, h: 32 })
  })

  it('redimensionne la zone par un coin en gardant le coin opposé fixe', () => {
    const c = { x: 100, y: 100, w: 400, h: 300 }
    expect(resizeCrop(c, 'se', 700, 600, 1920, 1080)).toEqual({ x: 100, y: 100, w: 600, h: 500 })
    expect(resizeCrop(c, 'nw', 50, 40, 1920, 1080)).toEqual({ x: 50, y: 40, w: 450, h: 360 })
    expect(resizeCrop(c, 'nw', 900, 900, 1920, 1080)).toMatchObject({ x: 500, y: 400 })
  })

  it("annonce la copie ou le réencodage avant l'export", () => {
    expect(planLabel({ copy: true, reason: null })).toMatch(/sans réencodage/)
    expect(planLabel({ copy: false, reason: 'recadrage' })).toBe('Réencodage : recadrage')
  })
})
