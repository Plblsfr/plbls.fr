import { describe, expect, it } from 'vitest'
import { bytes, episodeCode, filterReleases, filterTitles, releaseQuery, since, splitMissing, titleState, trackRequest, type ArrMissing, type ArrQueueItem, type ArrRelease, type ArrTitle } from './arr'

const film = (over: Partial<ArrTitle> = {}): ArrTitle => ({ service: 'radarr', id: 4, tmdb_id: 101, title: 'Film A', year: 2020, monitored: true, profile: 7, folder: '/films', status: 'released', size: 0, has_file: false, ...over })
const serie = (over: Partial<ArrTitle> = {}): ArrTitle => ({ service: 'sonarr', id: 3, tmdb_id: 201, title: 'La Série', year: 2019, monitored: true, profile: 3, folder: '/tv', status: 'ended', size: 10, episodes: 12, episode_files: 8, seasons: [], ...over })
const enFile = (over: Partial<ArrQueueItem> = {}): ArrQueueItem => ({ service: 'radarr', id: 55, title_id: 4, tmdb_id: 101, title: 'Film A', year: 2020, episode: null, release: 'Film.A.1080p', quality: 'Bluray-1080p', indexer: 'i', client: 'c', protocol: 'torrent', size: 1024 ** 3, left: 0, progress: 60, state: 'downloading', messages: [], eta: null, importable: true, ...over })
const release = (over: Partial<ArrRelease> = {}): ArrRelease => ({ guid: 'g', indexer_id: 1, title: 'R.1080p', indexer: 'i', protocol: 'torrent', quality: 'WEBDL-1080p', languages: ['French'], formats: [], score: 0, size: 100, seeders: 5, leechers: 0, age: 3, approved: true, rejections: [], full_season: false, ...over })

describe('arr', () => {
  it('formate les tailles, les épisodes et les dates', () => {
    expect(bytes(0)).toBe('')
    expect(bytes(850 * 1024 ** 2)).toBe('850 Mo')
    expect(bytes(11.4 * 1024 ** 3)).toBe('11,4 Go')
    expect(episodeCode({ season: 1, number: 4 })).toBe('S01E04')
    expect(episodeCode(null)).toBe('')
    const maintenant = Date.parse('2026-11-19T12:00:00Z')
    expect(since('2026-11-19T10:00:00Z', maintenant)).toBe('il y a 2 heures')
    expect(since(null, maintenant)).toBe('jamais')
  })

  it('adresse la recherche interactive au film, à la saison ou à l’épisode', () => {
    expect(releaseQuery({ service: 'radarr', title: 4, label: '' })).toBe('title=4')
    expect(releaseQuery({ service: 'sonarr', title: 3, season: 2, label: '' })).toBe('title=3&season=2')
    expect(releaseQuery({ service: 'sonarr', episode: 40, label: '' })).toBe('episode=40')
  })

  it('suit une demande acceptée dans la file', () => {
    const demande = { kind: 'movie' as const, tmdb_id: 101, seasons: null, state: 'acceptee' as const, sent: true }
    expect(trackRequest({ ...demande, sent: false }, [film()], [enFile()])).toEqual({ kind: 'none' })
    expect(trackRequest(demande, [film()], [enFile()])).toEqual({ kind: 'downloading', progress: 60, note: 'Bluray-1080p · 1 Go · téléchargement' })
    expect(trackRequest(demande, [film()], [enFile({ state: 'blocked', messages: ['Aucun fichier vidéo'] })])).toEqual({ kind: 'blocked', note: 'Aucun fichier vidéo' })
    expect(trackRequest(demande, [film()], [])).toEqual({ kind: 'missing', note: 'Aucun téléchargement en cours.' })
    expect(trackRequest(demande, [film({ has_file: true })], [])).toEqual({ kind: 'none' })
    expect(trackRequest(demande, [], [])).toEqual({ kind: 'none' })
  })

  it('ne compte pour une demande de saisons que les épisodes de ces saisons', () => {
    const demande = { kind: 'series' as const, tmdb_id: 201, seasons: [2], state: 'acceptee' as const, sent: true }
    const episode = (season: number, progress: number) => enFile({ service: 'sonarr', title_id: 3, tmdb_id: 201, progress, episode: { id: season, season, number: 1, title: null } })
    expect(trackRequest(demande, [serie()], [episode(1, 90)])).toEqual({ kind: 'missing', note: 'Aucun téléchargement en cours.' })
    expect(trackRequest(demande, [serie()], [episode(1, 90), episode(2, 20), episode(2, 40)])).toEqual({ kind: 'downloading', progress: 30, note: '2 téléchargements dans la file' })
  })

  it('résume l’état d’un titre', () => {
    expect(titleState(film())).toEqual({ label: 'Manquant', tone: 'att' })
    expect(titleState(film({ monitored: false }))).toEqual({ label: 'Non surveillé', tone: '' })
    expect(titleState(film(), [enFile()])).toEqual({ label: 'En cours', tone: 'info' })
    expect(titleState(serie())).toEqual({ label: '4 manquants', tone: 'att' })
    expect(titleState(serie({ episode_files: 12 }))).toEqual({ label: 'Complète', tone: 'ok' })
  })

  it('regroupe les épisodes manquants par série et par saison', () => {
    const ep = (season: number, number: number): ArrMissing => ({ service: 'sonarr', title_id: 3, tmdb_id: 201, title: 'La Série', year: 2019, last_search: null, episode: { id: season * 10 + number, season, number, title: null } })
    const { movies, groups } = splitMissing([ep(1, 4), { service: 'radarr', title_id: 4, tmdb_id: 101, title: 'Film A', year: 2020, last_search: null }, ep(2, 1), ep(1, 2)])
    expect(movies.map((m) => m.title)).toEqual(['Film A'])
    expect(groups.map((g) => [g.title, g.season, g.episodes.map((e) => e.episode?.number)])).toEqual([['La Série (2019)', 1, [2, 4]], ['La Série (2019)', 2, [1]]])
  })

  it('filtre et trie les releases', () => {
    const liste = [release({ guid: 'a', seeders: 2, size: 900 }), release({ guid: 'b', approved: false, quality: 'Remux-2160p', title: 'R.2160p', languages: ['English'], seeders: 50 }), release({ guid: 'c', seeders: 9, size: 50, age: 1 })]
    const ids = (f: Parameters<typeof filterReleases>[1]) => filterReleases(liste, f).map((r) => r.guid)
    const tout = { approved: '' as const, resolution: '', language: '', sort: 'score' as const }
    expect(ids(tout)).toEqual(['a', 'b', 'c'])
    expect(ids({ ...tout, approved: 'yes' })).toEqual(['a', 'c'])
    expect(ids({ ...tout, resolution: '2160' })).toEqual(['b'])
    expect(ids({ ...tout, language: 'French', sort: 'seeders' })).toEqual(['c', 'a'])
    expect(ids({ ...tout, sort: 'size' })).toEqual(['a', 'b', 'c'])
    expect(ids({ ...tout, sort: 'age' })).toEqual(['c', 'a', 'b'])
  })

  it('filtre le catalogue', () => {
    const liste = [serie(), film({ has_file: true }), film({ id: 5, title: 'Autre film', monitored: false })]
    const noms = (f: Parameters<typeof filterTitles>[1]) => filterTitles(liste, f).map((t) => t.title)
    expect(noms({ service: '', state: '', text: '' })).toEqual(['Autre film', 'Film A', 'La Série'])
    expect(noms({ service: 'radarr', state: 'unmonitored', text: '' })).toEqual(['Autre film'])
    expect(noms({ service: '', state: 'missing', text: '' })).toEqual(['La Série'])
    expect(noms({ service: '', state: '', text: 'film a' })).toEqual(['Film A'])
  })
})
