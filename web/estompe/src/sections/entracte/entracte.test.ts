import { describe, expect, it, vi } from 'vitest'
import { entracteApi } from '../../lib/api'
import { platform } from '../../lib/platform'
import { entracte } from './entracte.svelte'
import { failedSends, rubriqueActive, rubriques, requestSteps, sourceLabel, freeSpace, initialChoice, introAnalysed, introLabel, introPending, introTone, freshNotifications, requestBody, requestableSeasons, seasonsLabel, sections, splitSearch, stateLabel, stateTone, type IntroSeason, type IntroSeries, type RequestableSeries, type SearchResult, type SendOptions } from './entracte'

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

describe('relevé des notifications', () => {
  it('deux start() rapprochés ne laissent qu’un relevé périodique, et stop() pendant l’attente l’annule', async () => {
    vi.spyOn(entracteApi, 'notifications').mockResolvedValue({ items: [], last_id: 0 })
    const push = platform.notifications.push
    platform.notifications.push = null
    const actifs = new Set<unknown>()
    const pose = vi.spyOn(globalThis, 'setInterval').mockImplementation(((() => { const t = {}; actifs.add(t); return t }) as unknown) as typeof setInterval)
    const retire = vi.spyOn(globalThis, 'clearInterval').mockImplementation(((t: unknown) => { actifs.delete(t) }) as typeof clearInterval)
    await Promise.all([entracte.start(), entracte.start()])
    expect(actifs.size).toBe(1)
    const fin = entracte.start()
    entracte.stop()
    await fin
    expect(actifs.size).toBe(0)
    platform.notifications.push = push
    pose.mockRestore()
    retire.mockRestore()
    vi.restoreAllMocks()
  })
})

describe('réglages d’envoi à Radarr ou Sonarr', () => {
  const options = (defaults: SendOptions['defaults']): SendOptions => ({
    folders: [{ path: '/films', free: 5 * 1024 ** 3 }, { path: '/films-4k', free: null }],
    profiles: [{ id: 1, name: 'Any' }, { id: 6, name: 'HD-1080p' }],
    tags: [{ id: 3, label: 'vf' }],
    defaults,
  })

  it('part des réglages habituels du service', () => {
    expect(initialChoice(options({ folder: '/films-4k', profile: 6, tags: [3], availability: 'released' }))).toEqual({ folder: '/films-4k', profile: 6, tags: [3] })
  })

  it('se rabat sur les premiers proposés et ignore un tag disparu', () => {
    expect(initialChoice(options({ folder: '/ailleurs', profile: null, tags: [8], availability: null }))).toEqual({ folder: '/films', profile: 1, tags: [] })
  })

  it('ne propose rien sans emplacement ou sans profil', () => {
    expect(initialChoice({ ...options({ folder: null, profile: null, tags: [], availability: null }), folders: [] })).toBeNull()
  })

  it('affiche l’espace libre', () => {
    expect(freeSpace(5 * 1024 ** 3)).toBe('5 Go libres')
    expect(freeSpace(1536 * 1024 ** 3)).toBe('1,5 To libres')
    expect(freeSpace(null)).toBe('')
  })
})

describe('états de la détection des intros', () => {
  const saison = (over: Partial<IntroSeason> = {}): IntroSeason => ({ number: 1, episodes: 13, found: 0, state: 'jamais', error: null, updated_at: null, ...over })
  const uneSerie = (...seasons: IntroSeason[]): IntroSeries => ({ id: 's', title: 'Les Veilleurs', year: 2019, seasons })

  it('nomme chaque état en français', () => {
    expect((['jamais', 'en_file', 'en_cours', 'rien', 'erreur'] as const).map((state) => introLabel(saison({ state }))))
      .toEqual(['Jamais analysée', 'En file', 'Analyse en cours', 'Rien trouvé', 'Erreur'])
    expect(introLabel(saison({ state: 'trouvee', found: 12 }))).toBe('Intro trouvée (12 sur 13)')
    expect([introTone('trouvee'), introTone('erreur'), introTone('en_file'), introTone('en_cours'), introTone('rien'), introTone('jamais')]).toEqual(['ok', 'dgr', 'info', 'info', '', ''])
  })

  it('sait si une série attend son analyse ou en a déjà une', () => {
    expect(introPending(uneSerie(saison(), saison({ state: 'en_file' })))).toBe(true)
    expect(introPending(uneSerie(saison({ state: 'en_cours' })))).toBe(true)
    expect(introPending(uneSerie(saison({ state: 'trouvee' }), saison({ state: 'rien' })))).toBe(false)
    expect(introAnalysed(uneSerie(saison(), saison()))).toBe(false)
    expect(introAnalysed(uneSerie(saison(), saison({ state: 'erreur' })))).toBe(true)
  })
})

describe('rubriques', () => {
  const n = { movies: 12, series: 3, pending: 2 }
  it('quatre rubriques sans permission, « Mes demandes » et « Administration » selon les droits', () => {
    expect(rubriques(undefined, n).map((r) => r.label)).toEqual(['Accueil', 'Films', 'Séries', 'Historique'])
    expect(rubriques({ access: true, request: true, admin: false }, n).map((r) => r.id)).toEqual([undefined, 'movies', 'series', 'seen', 'requests'])
    const admin = rubriques({ access: true, request: false, admin: true }, n).at(-1)!
    expect(admin).toMatchObject({ id: 'admin', count: 2, alerte: true })
    expect(rubriques({ access: true, request: false, admin: true }, { ...n, pending: 0 }).at(-1)!.alerte).toBe(false)
  })
  it('la recherche éclaire « Mes demandes », « Reprendre » éclaire l’accueil', () => {
    expect(rubriqueActive('requests', 'search')).toBe(true)
    expect(rubriqueActive('requests', 'requests')).toBe(true)
    expect(rubriqueActive(undefined, undefined)).toBe(true)
    expect(rubriqueActive(undefined, 'continue')).toBe(true)
    expect(rubriqueActive(undefined, 'movies')).toBe(false)
    expect(rubriqueActive('movies', 'movies')).toBe(true)
  })
})

describe('suivi des demandes', () => {
  it('jauge en trois étapes selon l’état', () => {
    expect(requestSteps('en_attente')).toEqual(['fait', 'vide', 'vide'])
    expect(requestSteps('acceptee')).toEqual(['fait', 'fait', 'vide'])
    expect(requestSteps('disponible')).toEqual(['ok', 'ok', 'ok'])
    expect(requestSteps('refusee')).toEqual(['refus', 'vide', 'vide'])
  })
  it('envois en échec : acceptées, non envoyées, avec une erreur', () => {
    const base = { kind: 'movie' as const, tmdb_id: 1, seasons: null, title: 'T', year: null, poster_url: null, reason: null, created_at: '', updated_at: '' }
    const items = [
      { ...base, id: 'a', state: 'acceptee' as const, sent: false, send_error: 'dossier introuvable' },
      { ...base, id: 'b', state: 'acceptee' as const, sent: true, send_error: null },
      { ...base, id: 'c', state: 'en_attente' as const, send_error: 'x' },
    ]
    expect(failedSends(items).map((r) => r.id)).toEqual(['a'])
  })
  it('nom des sources de synchronisation', () => {
    expect(sourceLabel('tmdb')).toBe('TMDB')
    expect(sourceLabel('radarr')).toBe('Radarr')
    expect(sourceLabel('autre')).toBe('autre')
  })
})
