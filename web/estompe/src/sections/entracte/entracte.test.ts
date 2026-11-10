import { describe, expect, it } from 'vitest'
import { freshNotifications, requestBody, requestableSeasons, seasonsLabel, sections, splitSearch, stateLabel, stateTone, type RequestableSeries, type SearchResult } from './entracte'

const serie = (seasons: Partial<RequestableSeries['seasons'][number]>[]): RequestableSeries => ({
  tmdb_id: 201, title: 'La Série', year: 2019, poster_url: null,
  seasons: seasons.map((s, i) => ({ number: i + 1, title: `Saison ${i + 1}`, episode_count: 8, available_count: 0, complete: false, request_state: null, ...s })),
})

describe('demandes', () => {
  it('libellés et tons des états', () => {
    expect(stateLabel('en_attente')).toBe('En attente')
    expect(stateLabel('refusee')).toBe('Refusée')
    expect(stateTone('disponible')).toBe('ok')
    expect(stateTone('refusee')).toBe('dgr')
    expect(stateTone('en_attente')).toBe('')
  })
  it('portée de la demande', () => {
    expect(seasonsLabel({ kind: 'movie', seasons: null })).toBe('Film')
    expect(seasonsLabel({ kind: 'series', seasons: null })).toBe('Série entière')
    expect(seasonsLabel({ kind: 'series', seasons: [2] })).toBe('Saison 2')
    expect(seasonsLabel({ kind: 'series', seasons: [1, 3] })).toBe('Saisons 1, 3')
  })
  it('saisons demandables : ni complètes ni déjà demandées', () => {
    const s = serie([{ complete: true, available_count: 8 }, {}, { request_state: 'en_attente' }])
    expect(requestableSeasons(s).map((x) => x.number)).toEqual([2])
  })
  it('corps : film, série entière, ou saisons précises (saison manquante d’une série présente)', () => {
    expect(requestBody('movie', 103, [])).toEqual({ kind: 'movie', tmdb_id: 103 })
    const absente = serie([{}, {}])
    expect(requestBody('series', 201, [1, 2], absente)).toEqual({ kind: 'series', tmdb_id: 201 })
    expect(requestBody('series', 201, [2], absente)).toEqual({ kind: 'series', tmdb_id: 201, seasons: [2] })
    const presente = serie([{ complete: true, available_count: 8 }, {}])
    expect(requestBody('series', 201, [2], presente)).toEqual({ kind: 'series', tmdb_id: 201, seasons: [2] })
    expect(requestBody('series', 201, [3, 2, 2], absente)).toEqual({ kind: 'series', tmdb_id: 201, seasons: [2, 3] })
  })
})

describe('recherche', () => {
  const r = (title: string, present: boolean): SearchResult => ({ id: present ? title : null, kind: 'movie', title, year: 2020, present, tmdb_id: title.length, requestable: !present })
  it('présents puis absents, distingués', () => {
    const { present, absent } = splitSearch([r('A', true), r('Absent', false), r('B', true)])
    expect(present.map((x) => x.title)).toEqual(['A', 'B'])
    expect(absent.map((x) => x.title)).toEqual(['Absent'])
  })
})

describe('notifications sans push (app Windows)', () => {
  it('ne montre que les nouvelles, dans l’ordre', () => {
    const n = (id: number) => ({ id, kind: 'disponible', request_id: null, title: `t${id}`, body: '', created_at: '' })
    expect(freshNotifications([n(5), n(3), n(4)], 3).map((x) => x.id)).toEqual([4, 5])
    expect(freshNotifications([n(1)], 1)).toEqual([])
  })
})

describe('sections de l’app', () => {
  const me = (access: boolean) => ({ sub: 's', name: 'n', email: '', permissions: { access, request: false, admin: false }, push: { web: false, fcm: false } })
  it('chacune selon sa permission', () => {
    expect(sections(true, null)).toEqual({ messagerie: true, entracte: false })
    expect(sections(false, me(true))).toEqual({ messagerie: false, entracte: true })
    expect(sections(true, me(true))).toEqual({ messagerie: true, entracte: true })
    expect(sections(false, me(false))).toEqual({ messagerie: false, entracte: false })
  })
})
