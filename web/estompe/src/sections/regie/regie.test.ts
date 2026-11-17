import { describe, expect, it, vi } from 'vitest'
import { eta, filterTorrents, groupCounts, limitBytes, limitInput, percent, torrentLabel, torrentState, bytes, indexerState, problems, serviceState, sortIndexers, speed, type Downloads, type Indexer, type RegieStatus, type Service, type Torrent } from './regie'
import { Suivi } from './regie.svelte'
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
})
