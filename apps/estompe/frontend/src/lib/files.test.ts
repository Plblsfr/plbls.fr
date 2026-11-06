import { describe, expect, it } from 'vitest'
import { checkSize, fitWithin, levelOf, recordingType, seekRatio, swapExt, toWaveform } from './files'
import { continues, formatSize } from './format'

describe('fitWithin', () => {
  it('réduit en gardant les proportions', () => {
    expect(fitWithin(4000, 2000)).toEqual({ w: 1920, h: 960 })
    expect(fitWithin(1000, 3000)).toEqual({ w: 640, h: 1920 })
  })
  it('n\'agrandit jamais', () => {
    expect(fitWithin(800, 600)).toEqual({ w: 800, h: 600 })
  })
  it('reste au moins à 1 px', () => {
    expect(fitWithin(100000, 1).h).toBe(1)
  })
})

describe('swapExt', () => {
  it('change l\'extension', () => {
    expect(swapExt('photo.png', 'webp')).toBe('photo.webp')
    expect(swapExt('a.b.jpeg', 'webp')).toBe('a.b.webp')
    expect(swapExt('sans', 'webp')).toBe('sans.webp')
    expect(swapExt('.cache', 'webp')).toBe('.cache.webp')
  })
})

describe('checkSize', () => {
  it('refuse au-delà de 10 Mo', () => {
    expect(checkSize(10 * 1024 * 1024)).toBeNull()
    expect(checkSize(10 * 1024 * 1024 + 1)).toMatch(/10 Mo/)
  })
})

describe('format', () => {
  it('formate les tailles', () => {
    expect(formatSize(12)).toBe('12 o')
    expect(formatSize(1536)).toBe('1.5 Ko')
    expect(formatSize(50 * 1024)).toBe('50 Ko')
    expect(formatSize(3.5 * 1024 * 1024)).toBe('3.5 Mo')
  })
  it('regroupe les messages proches du même auteur', () => {
    const a = { author: { id: 'u1' }, created_at: '2026-10-13T10:00:00Z' }
    expect(continues(a, { author: { id: 'u1' }, created_at: '2026-10-13T10:04:00Z' })).toBe(true)
    expect(continues(a, { author: { id: 'u1' }, created_at: '2026-10-13T10:06:00Z' })).toBe(false)
    expect(continues(a, { author: { id: 'u2' }, created_at: '2026-10-13T10:01:00Z' })).toBe(false)
    expect(continues(undefined, a)).toBe(false)
  })
})

describe('recordingType', () => {
  it('préfère Opus dans WebM, puis Ogg, puis AAC dans MP4 (Safari)', () => {
    expect(recordingType(() => true)).toEqual({ mime: 'audio/webm;codecs=opus', ext: 'webm' })
    expect(recordingType((t) => t === 'audio/ogg;codecs=opus')).toEqual({ mime: 'audio/ogg;codecs=opus', ext: 'ogg' })
    expect(recordingType((t) => t === 'audio/mp4')).toEqual({ mime: 'audio/mp4', ext: 'm4a' })
    expect(recordingType(() => false)).toBeNull()
  })
})

describe('levelOf', () => {
  it('mesure le silence à 0 et un signal plein à 100', () => {
    expect(levelOf(new Float32Array(256))).toBe(0)
    expect(levelOf(new Float32Array(256).fill(1))).toBe(100)
  })
})

describe('toWaveform', () => {
  it('réduit par moyenne de tranches, sans dépasser le nombre demandé', () => {
    expect(toWaveform([10, 30, 50, 70], 2)).toEqual([20, 60])
    expect(toWaveform([40, 40], 40)).toEqual([40, 40])
  })
  it('renvoie toujours au moins une valeur (le serveur en exige une)', () => {
    expect(toWaveform([])).toEqual([0])
  })
})

describe('seekRatio', () => {
  it('se place selon le clic dans la barre, pas dans l\'élément enfant touché', () => {
    expect(seekRatio(150, 100, 200)).toBe(0.25)
    expect(seekRatio(100, 100, 200)).toBe(0)
    expect(seekRatio(300, 100, 200)).toBe(1)
  })
  it('borne entre 0 et 1 et gère une largeur nulle', () => {
    expect(seekRatio(50, 100, 200)).toBe(0)
    expect(seekRatio(900, 100, 200)).toBe(1)
    expect(seekRatio(150, 100, 0)).toBe(0)
  })
})
