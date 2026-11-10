import { describe, expect, it } from 'vitest'
import { absolute, clock, needsReopen, savedQuality, saveQuality, support, episodeLabel, fullTitle, percent, pickEpisode, reasonLabel, remaining, resumeAt, saveDue, shortDuration, tint, trackName, yearOptions, type FilmItem } from './films'

const item = (over: Partial<FilmItem> = {}): FilmItem => ({
  id: 'a', kind: 'episode', title: 'La Combe', year: 2024, overview: null, duration: 3000, genres: [], season: 2, episode: 4,
  series_id: 's', series_title: 'Les Veilleurs', poster: false, backdrop: false, progress: null, ...over,
})
const seen = { position: 2900, duration: 3000, seen: true }
const started = { position: 600, duration: 3000, seen: false }

describe('formatage', () => {
  it('horloge', () => {
    expect(clock(0)).toBe('0:00')
    expect(clock(65)).toBe('1:05')
    expect(clock(3972)).toBe('1:06:12')
    expect(clock(-3)).toBe('0:00')
  })
  it('durée courte et reste', () => {
    expect(shortDuration(2880)).toBe('48 min')
    expect(shortDuration(6720)).toBe('1 h 52')
    expect(remaining(started)).toBe('reste 40 min')
    expect(remaining({ position: 4000, duration: 3000, seen: true })).toBe('reste 0 min')
  })
  it('pourcentage borné', () => {
    expect(percent(null)).toBe('0%')
    expect(percent(started)).toBe('20%')
    expect(percent({ position: 9, duration: 3, seen: true })).toBe('100%')
    expect(percent({ position: 9, duration: 0, seen: false })).toBe('0%')
  })
  it('titres', () => {
    expect(episodeLabel(item())).toBe('S2 · E4')
    expect(fullTitle(item())).toBe('Les Veilleurs · S2 · E4, La Combe')
    expect(fullTitle(item({ series_title: null, season: null, episode: null }))).toBe('La Combe')
  })
  it('noms de pistes et raisons', () => {
    expect(trackName({ n: 0, label: 'Français', language: 'fra' })).toBe('Français')
    expect(trackName({ n: 0, label: null, language: 'fra' })).toBe('fra')
    expect(trackName({ n: 1, label: null, language: null })).toBe('Piste 2')
    expect(reasonLabel('double_extension')).toBe('Double extension')
    expect(reasonLabel('inconnue')).toBe('inconnue')
  })
  it('filtre des années', () => {
    const o = yearOptions(2026)
    expect(o[0].value).toBe('')
    expect(o[1].value).toBe('2020-2026')
    expect(yearOptions(2019)[1].value).toBe('2020-2020')
  })
  it('couleur stable par titre', () => {
    expect(tint('abc')).toEqual(tint('abc'))
    expect(tint('')).toBeDefined()
  })
})

describe('progression', () => {
  it('enregistre toutes les 15 secondes', () => {
    expect(saveDue(0, 14_999)).toBe(false)
    expect(saveDue(0, 15_000)).toBe(true)
    expect(saveDue(100_000, 110_000)).toBe(false)
  })
  it('reprend une lecture commencée, pas une lecture vue', () => {
    expect(resumeAt(item({ progress: started }))).toBe(600)
    expect(resumeAt(item({ progress: seen }))).toBe(0)
    expect(resumeAt(item())).toBe(0)
  })
  it('choisit l\'épisode en cours, puis le premier non vu, puis le premier', () => {
    const e1 = item({ id: '1', progress: seen })
    const e2 = item({ id: '2' })
    const e3 = item({ id: '3', progress: started })
    expect(pickEpisode([e1, e2, e3])?.id).toBe('3')
    expect(pickEpisode([e1, e2])?.id).toBe('2')
    expect(pickEpisode([e1])?.id).toBe('1')
    expect(pickEpisode([])).toBeUndefined()
  })
})

describe('Entracte : formats, qualité et position', () => {
  it('annonce ce que le lecteur sait lire', () => {
    const ok = new Set(['video/mp4', 'video/webm', 'video/mp4; codecs="avc1.42E01E"', 'video/webm; codecs="vp9"', 'audio/mp4; codecs="mp4a.40.2"', 'audio/webm; codecs="opus"'])
    expect(support((m) => (ok.has(m) ? 'probably' : ''))).toEqual({ containers: ['mp4', 'webm'], video: ['h264', 'vp9'], audio: ['aac', 'opus'] })
    expect(support(() => '')).toEqual({ containers: [], video: [], audio: [] })
  })
  it('palier mémorisé par appareil, repli sur Originale', () => {
    const m = new Map<string, string>()
    const store = { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) }
    expect(savedQuality(store)).toBe('original')
    saveQuality(store, '720p')
    expect(savedQuality(store)).toBe('720p')
    expect(savedQuality(store, ['original', '480p'])).toBe('original')
    m.set('entracte.qualite', '4k')
    expect(savedQuality(store)).toBe('original')
    expect(savedQuality(null)).toBe('original')
    const casse = { getItem: () => { throw new Error('bloqué') }, setItem: () => { throw new Error('bloqué') } }
    expect(savedQuality(casse)).toBe('original')
    expect(() => saveQuality(casse, '480p')).not.toThrow()
  })
  it('position réelle d’un flux HLS qui commence plus loin', () => {
    expect(absolute(10, 600)).toBe(610)
    expect(needsReopen(550, 600, 120)).toBe(true)
    expect(needsReopen(650, 600, 120)).toBe(false)
    expect(needsReopen(900, 600, 120)).toBe(true)
  })
})
