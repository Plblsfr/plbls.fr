import { describe, expect, it } from 'vitest'
import { bytes, indexerState, problems, serviceState, sortIndexers, speed, type Indexer, type RegieStatus, type Service } from './regie'

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
})
