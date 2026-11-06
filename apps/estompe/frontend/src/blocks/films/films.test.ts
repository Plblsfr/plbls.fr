import { describe, expect, it } from 'vitest'
import { clock, directOk, episodeLabel, fullTitle, percent, pickEpisode, reasonLabel, remaining, resumeAt, saveDue, shortDuration, tint, trackName, yearOptions, type FilmItem } from './films'

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

describe('lecture directe', () => {
  it('dépend de ce que sait lire l\'appareil', () => {
    expect(directOk(() => 'probably')).toBe(true)
    expect(directOk(() => 'maybe')).toBe(true)
    expect(directOk(() => '')).toBe(false)
  })
})
