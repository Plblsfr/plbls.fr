import { describe, expect, it, vi } from 'vitest'
import { seedingTime, sheetGroups, sheetText, type Sheet, depositComplete, depositState, invalidFiles, matchFiles, eta, filterTorrents, groupCounts, limitBytes, limitInput, percent, torrentLabel, torrentState, bytes, indexerState, problems, serviceState, sortIndexers, speed, type Deposit, type DepositFile, type Downloads, type Indexer, type RegieStatus, type Service, type Torrent } from './regie'
import { Suivi, envoyerFichier } from './regie.svelte'
import { ApiError } from '../../lib/api'

const service = (s: Partial<Service>): Service => ({ key: 'radarr', name: 'Radarr', configured: true, reachable: true, version: '5.0.0', error: null, alerts: [], ...s })
const indexer = (i: Partial<Indexer>): Indexer => ({ id: 1, name: 'A', enabled: true, protocol: 'torrent', privacy: 'private', failing: false, disabled_until: null, last_failure: null, ...i })

describe('régie', () => {
  it('résume l’état d’un service par son pire signal', () => {
    expect(serviceState(service({}))).toEqual({ label: 'OK', tone: 'ok' })
    expect(serviceState(service({ configured: false, reachable: false })).label).toBe('Non configuré')
    expect(serviceState(service({ reachable: false, error: 'injoignable' }))).toEqual({ label: 'Injoignable', tone: 'dgr' })
    expect(serviceState(service({ alerts: [{ level: 'warning', source: 'x', message: 'm' }] })).tone).toBe('att')
    expect(serviceState(service({ alerts: [{ level: 'warning', source: 'x', message: 'm' }, { level: 'error', source: 'y', message: 'n' }] })).tone).toBe('dgr')
    expect(serviceState(service({ alerts: [{ level: 'notice', source: 'x', message: 'm' }] })).tone).toBe('ok')
    expect(serviceState(service({ error: 'file : HTTP 500' })).tone).toBe('att')
  })

  it('classe les indexeurs : en panne, actifs, désactivés', () => {
    const tries = sortIndexers([indexer({ id: 1, name: 'Zèbre' }), indexer({ id: 2, name: 'Éteint', enabled: false, failing: true }), indexer({ id: 3, name: 'Panne', failing: true }), indexer({ id: 4, name: 'Alpha' })])
    expect(tries.map((i) => i.name)).toEqual(['Panne', 'Alpha', 'Zèbre', 'Éteint'])
    expect(indexerState(tries[0]).label).toBe('En panne')
    expect(indexerState(tries[3]).label).toBe('Désactivé')
  })

  it('formate tailles et débits', () => {
    expect(bytes(null)).toBe('')
    expect(bytes(-1)).toBe('')
    expect(bytes(512)).toBe('512 o')
    expect(bytes(1536)).toBe('1,5 Ko')
    expect(bytes(250 * 1024 ** 3)).toBe('250 Go')
    expect(bytes(3 * 1024 ** 4)).toBe('3 To')
    expect(speed(0)).toBe('0 o/s')
    expect(speed(2048)).toBe('2 Ko/s')
  })

  it('compte ce qui demande une action', () => {
    const etat: RegieStatus = {
      services: [service({}), service({ key: 'sonarr', reachable: false }), service({ key: 'lidarr', configured: false, reachable: false })],
      indexers: [indexer({ failing: true }), indexer({ id: 2, enabled: false, failing: true })],
      disks: [{ service: 'radarr', path: '/films', free: 1, accessible: true }, { service: 'sonarr', path: '/series', free: null, accessible: false }],
      queue: [{ service: 'radarr', id: 7, title: 'x', level: 'warning', state: null, messages: [] }],
    }
    expect(problems(etat)).toBe(4)
  })

  const torrent = (t: Partial<Torrent>): Torrent => ({ hash: 'a'.repeat(40), name: 'Film.2024.1080p', state: 'downloading', progress: 0.5, size: 1, download_speed: 0, upload_speed: 0, eta: 0, ratio: 0, category: '', tags: [], added_at: 0, seeds: 0, peers: 0, download_limit: 0, upload_limit: 0, title: null, ...t })

  it('range les états de qBittorrent 4 et 5 dans les mêmes groupes', () => {
    expect(torrentState('pausedDL').group).toBe('stopped')
    expect(torrentState('stoppedUP')).toEqual({ group: 'stopped', label: 'Terminé, suspendu', tone: '' })
    expect(torrentState('stalledUP').group).toBe('seeding')
    expect(torrentState('missingFiles').tone).toBe('dgr')
    expect(torrentState('checkingResumeData').group).toBe('checking')
    expect(torrentState('nouveau').label).toBe('nouveau')
    const liste = [torrent({ state: 'downloading' }), torrent({ state: 'stalledUP' }), torrent({ state: 'stoppedDL' }), torrent({ state: 'error' })]
    expect(groupCounts(liste)).toEqual({ downloading: 1, seeding: 1, stopped: 1, checking: 0, error: 1 })
  })

  it('nomme un torrent par son titre rattaché et le filtre', () => {
    const libre = torrent({ name: 'Ubuntu.iso', category: 'divers', tags: ['linux'] })
    const lie = torrent({ hash: 'b'.repeat(40), state: 'stalledUP', title: { service: 'sonarr', title: 'La Série', detail: 'S01E02 · Pilote' } })
    expect(torrentLabel(libre)).toEqual({ title: 'Ubuntu.iso', sub: '' })
    expect(torrentLabel(lie)).toEqual({ title: 'La Série · S01E02 · Pilote', sub: 'Sonarr · Film.2024.1080p' })
    expect(filterTorrents([libre, lie], '', '')).toHaveLength(2)
    expect(filterTorrents([libre, lie], 'seeding', '')).toEqual([lie])
    expect(filterTorrents([libre, lie], '', 'série pilote')).toEqual([lie])
    expect(filterTorrents([libre, lie], '', 'LINUX')).toEqual([libre])
    expect(filterTorrents([libre, lie], 'seeding', 'ubuntu')).toEqual([])
  })

  it('formate durée restante, pourcentage et limites', () => {
    expect(eta(8_640_000)).toBe('')
    expect(eta(0)).toBe('')
    expect(eta(30)).toBe('< 1 min')
    expect(eta(600)).toBe('10 min')
    expect(eta(3900)).toBe('1 h 05')
    expect(eta(90_000)).toBe('1 j 1 h')
    expect(seedingTime(undefined)).toBe('')
    expect(seedingTime(59)).toBe('')
    expect(seedingTime(2700)).toBe('45 min')
    expect(seedingTime(11_100)).toBe('3 h 05')
    expect(seedingTime(187_200)).toBe('2 j 4 h')
    expect(percent(0.9999)).toBe('99,9 %')
    expect(percent(1)).toBe('100 %')
    expect(limitBytes('')).toBe(0)
    expect(limitBytes('1,5')).toBe(1536)
    expect(limitBytes('abc')).toBeNull()
    expect(limitBytes('-1')).toBeNull()
    expect(limitInput(0)).toBe('')
    expect(limitInput(2048)).toBe('2')
  })

  it('enchaîne les lectures avec la dernière version, garde la liste sur une erreur de qBittorrent et s’arrête', async () => {
    const appels: (number | undefined)[] = []
    const reponses: (Downloads | Error)[] = [
      { version: 1, torrents: [torrent({})], error: null },
      new Error('réseau coupé'),
      { version: 2, error: 'injoignable' },
    ]
    let fin = () => {}
    const fini = new Promise<void>((r) => (fin = r))
    const pauses: number[] = []
    const suivi = new Suivi(async (since) => {
      appels.push(since)
      const r = reponses.shift()
      if (!r) { suivi.stop(); fin(); throw new ApiError(0, 'timeout', 'fin') }
      if (r instanceof Error) throw r
      return r
    }, async (ms) => { pauses.push(ms) })
    suivi.start()
    await fini
    expect(appels).toEqual([undefined, 1, 1, 2])
    expect(pauses).toEqual([1000])
    expect(suivi.data?.version).toBe(2)
    expect(suivi.data?.torrents).toHaveLength(1)
    expect(suivi.data?.error).toBe('injoignable')
  })

  it('arrête le suivi quand la permission est refusée', async () => {
    let appels = 0
    const suivi = new Suivi(async () => { appels++; throw new ApiError(403, 'forbidden', 'permission regie:admin requise') })
    suivi.start()
    await vi.waitFor(() => expect(suivi.failed).toBe('permission regie:admin requise'))
    expect(appels).toBe(1)
  })

  const attendu = (index: number, path: string, size: number, plus: Partial<DepositFile> = {}): DepositFile => ({ index, path, size, received: 0, valid: null, ...plus })
  const depot = (d: Partial<Deposit>): Deposit => ({ id: 'a'.repeat(40), name: 'Film.2024', state: 'upload', created_at: 0, tracker: null, private: true, size: 8, received: 0, files: [attendu(0, 'Film.2024/film.mkv', 5), attendu(1, 'Film.2024/Subs/fr.srt', 3)], imported: [], error: null, ...d })

  it('associe les fichiers choisis à ceux du torrent par la taille, puis par le chemin', () => {
    const fichiers = [attendu(0, 'Serie/S01/E01.mkv', 100), attendu(1, 'Serie/S01/E02.mkv', 100), attendu(2, 'Serie/S01/info.nfo', 7), attendu(3, 'Serie/S01/autre.txt', 9)]
    const liens = matchFiles(fichiers, [
      { name: 'E02.mkv', size: 100, path: 'Mes séries/S01/E02.mkv' },
      { name: 'renomme.nfo', size: 7 },
      { name: 'E01.mkv', size: 100, path: 'Mes séries/S01/E01.mkv' },
      { name: 'autre.txt', size: 8 },
    ])
    expect([...liens]).toEqual([[0, 2], [1, 0], [2, 1]])
    expect(matchFiles(fichiers.slice(0, 2), [{ name: 'x.mkv', size: 100 }]).size).toBe(0)
    expect(matchFiles([attendu(0, 'seul.mkv', 4)], [{ name: 'Autre nom.mkv', size: 4 }]).get(0)).toBe(0)
  })

  it('résume l’état d’un dépôt', () => {
    expect(depositState(depot({})).label).toBe('Fichiers attendus')
    const recu = depot({ received: 8, files: [attendu(0, 'a', 5, { received: 5 }), attendu(1, 'b', 3, { received: 3 })] })
    expect(depositState(recu)).toEqual({ label: 'Prêt à vérifier', tone: 'info' })
    expect(depositComplete(recu)).toBe(true)
    expect(depositComplete(depot({}))).toBe(false)
    const ko = depot({ state: 'mismatch', files: [attendu(0, 'a', 5, { received: 5, valid: 1 }), attendu(1, 'b', 3, { received: 3, valid: 0.4 })] })
    expect(depositState(ko).tone).toBe('dgr')
    expect(invalidFiles(ko).map((f) => f.index)).toEqual([1])
    expect(depositState(depot({ state: 'seeding' })).label).toBe('En partage')
  })

  it('envoie un fichier par morceaux, reprend à la position du serveur et retente une coupure', async () => {
    const fichier = new Blob(['0123456789'])
    const appels: [number, number, boolean][] = []
    const suivis: number[] = []
    let coupe = true
    await envoyerFichier(fichier, 2, async (offset, morceau, reset) => {
      appels.push([offset, morceau.size, reset])
      if (offset === 2) throw new ApiError(409, 'bad_offset', 'position inattendue', { received: 4 })
      if (offset === 8 && coupe) { coupe = false; throw new TypeError('réseau') }
      return { received: offset + morceau.size }
    }, (r) => suivis.push(r), { taille: 4, pause: async () => {} })
    expect(appels).toEqual([[2, 4, false], [4, 4, false], [8, 2, false], [8, 2, false]])
    expect(suivis).toEqual([4, 8, 8, 10])
    expect(appels.every(([, taille]) => taille <= 4)).toBe(true)
  })

  it('remplace un fichier depuis le début et s’arrête sur un refus', async () => {
    const appels: [number, boolean][] = []
    await envoyerFichier(new Blob(['abcdef']), 6, async (offset, morceau, reset) => { appels.push([offset, reset]); return { received: offset + morceau.size } }, () => {}, { taille: 4, remplacer: true })
    expect(appels).toEqual([[0, true], [4, false]])
    await expect(envoyerFichier(new Blob(['abc']), 0, async () => { throw new ApiError(409, 'bad_state', 'ce dépôt n’accepte plus de fichier') }, () => {})).rejects.toThrow('n’accepte plus')
    await expect(envoyerFichier(new Blob(['abc']), 0, async () => ({ received: 3 }), () => {}, { arret: () => true })).rejects.toMatchObject({ code: 'cancelled' })
  })

  it('rédige la fiche technique d’un film', () => {
    const fiche: Sheet = {
      service: 'radarr', id: 4, title: 'Le Film', original_title: 'The Movie', artist: null, year: 2024, runtime: 136, genres: ['Action', 'Science-Fiction'],
      ids: { tmdb: 603, imdb: 'tt0133093', tvdb: null, musicbrainz: null },
      files: [{
        name: 'Le.Film.2024.1080p.BluRay.x264-GRP.mkv', group: null, size: 9 * 1024 ** 3, quality: 'Bluray-1080p', release_group: 'GRP', scene_name: 'Le.Film.2024.1080p.BluRay.x264-GRP', edition: null, languages: ['French', 'English'],
        video: { codec: 'x264', resolution: '1920x800', bit_depth: 8, dynamic_range: null, fps: 23.976, bitrate: 9_500_000, scan: 'Progressive' },
        audio: { codec: 'DTS', channels: 5.1, languages: 'fre/eng', bitrate: 1_509_000, streams: 2, sample_rate: null, bits: null },
        subtitles: 'fre/eng', duration: '2:16:17',
      }],
    }
    expect(sheetText(fiche)).toBe([
      'Le Film (2024)',
      'Titre original : The Movie',
      'Durée : 2 h 16 · Genres : Action, Science-Fiction',
      'TMDB : https://www.themoviedb.org/movie/603',
      'IMDb : https://www.imdb.com/title/tt0133093/',
      '',
      'Le.Film.2024.1080p.BluRay.x264-GRP.mkv',
      'Taille : 9 Go · Qualité : Bluray-1080p · Groupe : GRP',
      'Nom de release : Le.Film.2024.1080p.BluRay.x264-GRP',
      'Vidéo : x264, 1920x800, 8 bits, 23,976 i/s, 9,5 Mb/s, Progressive',
      'Audio : DTS 5.1, fre/eng, 1,5 Mb/s, 2 pistes',
      'Langues : French, English',
      'Sous-titres : fre/eng',
      'Durée : 2:16:17',
    ].join('\n'))
    expect(sheetGroups(fiche)).toEqual([])
  })

  it('limite la fiche d’une série à une saison et décrit un album', () => {
    const vide = { size: null, quality: null, release_group: null, scene_name: null, edition: null, languages: [], video: null, audio: null, subtitles: null, duration: null }
    const serie: Sheet = {
      service: 'sonarr', id: 2, title: 'La Série', original_title: null, artist: null, year: 2020, runtime: null, genres: [], ids: { tmdb: 12, imdb: null, tvdb: 77, musicbrainz: null },
      files: [{ ...vide, name: 'S01E01.mkv', group: 'Saison 1' }, { ...vide, name: 'S02E01.mkv', group: 'Saison 2' }],
    }
    expect(sheetGroups(serie)).toEqual(['Saison 1', 'Saison 2'])
    const texte = sheetText(serie, 'Saison 2')
    expect(texte).toContain('https://www.themoviedb.org/tv/12')
    expect(texte).toContain('TVDB : https://thetvdb.com/?tab=series&id=77')
    expect(texte).toContain('Saison 2\n\nS02E01.mkv')
    expect(texte).not.toContain('S01E01')
    const album: Sheet = {
      service: 'lidarr', id: 3, title: 'L’Album', original_title: null, artist: 'Le Groupe', year: 1999, runtime: null, genres: ['Rock'], ids: { tmdb: null, imdb: null, tvdb: null, musicbrainz: 'mbid-1' },
      files: [{ ...vide, name: '01 - Titre.flac', group: null, quality: 'FLAC', audio: { codec: 'FLAC', channels: 2, languages: null, bitrate: 1_011_000, streams: null, sample_rate: 44_100, bits: 16 } }],
    }
    const t = sheetText(album)
    expect(t.split('\n')[0]).toBe('Le Groupe - L’Album (1999)')
    expect(t).toContain('Audio : FLAC stéréo, 1 Mb/s, 44,1 kHz, 16 bits')
    expect(t).toContain('MusicBrainz : https://musicbrainz.org/release-group/mbid-1')
    expect(sheetText({ ...album, files: [] })).toContain('Aucun fichier.')
  })
})
