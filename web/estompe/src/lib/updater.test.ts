import { afterEach, describe, expect, it, vi } from 'vitest'
import { availability, isNewer, updater } from './updater.svelte'

describe('mise à jour', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    updater.available = null
    updater.refused = null
    updater.blocked = false
    updater.installing = false
    updater.countdown = 0
  })

  it('compare les versions numériquement', () => {
    expect(isNewer('0.10.0', '0.9.9')).toBe(true)
    expect(isNewer('1.0.0', '0.99.99')).toBe(true)
    expect(isNewer('0.3.1', '0.3.1')).toBe(false)
    expect(isNewer('0.3.0', '0.3.1')).toBe(false)
  })

  it('ne propose rien si on est à jour ou si la version installée est inconnue', () => {
    expect(availability('1.2.0', { version: '1.2.0', mandatory: true })).toBeNull()
    expect(availability('1.3.0', { version: '1.2.0', minVersion: '1.3.0' })).toBeNull()
    expect(availability('', { version: '1.2.0' })).toBeNull()
  })

  it('propose une mise à jour reportable par défaut', () => {
    expect(availability('1.1.0', { version: '1.2.0', notes: 'n' })).toMatchObject({ version: '1.2.0', force: false, notes: 'n' })
  })

  it('impose la mise à jour si la release est obligatoire, y compris par l\'ancien champ force', () => {
    expect(availability('1.1.0', { version: '1.2.0', mandatory: true, message: 'faille' })).toMatchObject({ force: true, message: 'faille' })
    expect(availability('1.1.0', { version: '1.2.0', force: true })?.force).toBe(true)
  })

  it('impose la mise à jour sous la version minimale, même après plusieurs releases sautées', () => {
    expect(availability('0.5.0', { version: '1.2.0', minVersion: '1.0.0' })?.force).toBe(true)
    expect(availability('1.0.0', { version: '1.2.0', minVersion: '1.0.0' })?.force).toBe(false)
  })

  it('lit web.json, propose la mise à jour et ne bloque pas un utilisateur occupé avant le délai', async () => {
    vi.useFakeTimers()
    const input = document.createElement('textarea')
    document.body.append(input)
    input.value = 'brouillon'
    input.focus()
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ version: '999.0.0', mandatory: true, message: 'urgent' }))))
    await updater.check()
    expect(updater.available).toMatchObject({ version: '999.0.0', force: true, message: 'urgent' })
    expect(updater.blocked).toBe(false)
    expect(updater.countdown).toBe(120)
    vi.advanceTimersByTime(120_000)
    expect(updater.blocked).toBe(true)
    input.remove()
    vi.useRealTimers()
  })

  it('bloque aussitôt un utilisateur qui n\'écrit pas', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ version: '999.0.0', minVersion: '998.0.0' }))))
    await updater.check()
    expect(updater.blocked).toBe(true)
  })

  it('traduit le refus 426 de l\'API en écran de mise à jour', () => {
    vi.spyOn(updater, 'check').mockResolvedValue()
    updater.require('version trop ancienne')
    expect(updater.refused).toBe('version trop ancienne')
    expect(updater.forced).toBe(true)
    expect(updater.blocked).toBe(true)
  })
  it('ne programme les vérifications qu’une fois, même si start() est rappelé', () => {
    const minuterie = vi.spyOn(globalThis, 'setInterval')
    const ecoute = vi.spyOn(document, 'addEventListener')
    updater.start()
    updater.start()
    expect(minuterie.mock.calls.filter(([, ms]) => ms === 30 * 60_000)).toHaveLength(1)
    expect(ecoute.mock.calls.filter(([t]) => t === 'visibilitychange')).toHaveLength(1)
    minuterie.mockRestore()
    ecoute.mockRestore()
  })
})
