import { describe, it, expect, vi } from 'vitest'
import { pushState, setupNotifications } from './push'
import { prefs } from '../../lib/prefs.svelte'
import { parseHash } from '../../lib/route.svelte'

const base = { supported: true, subscribed: false, denied: false, serverOff: false }

describe('pushState', () => {
  it('actif seulement si abonné et rien ne bloque', () => {
    expect(pushState({ ...base, subscribed: true })).toBe('on')
    expect(pushState(base)).toBe('off')
  })
  it('un blocage l\'emporte sur l\'abonnement', () => {
    expect(pushState({ ...base, subscribed: true, denied: true })).toBe('denied')
    expect(pushState({ ...base, serverOff: true })).toBe('unavailable')
    expect(pushState({ ...base, supported: false, denied: true })).toBe('unsupported')
  })
})

describe('setupNotifications', () => {
  it('sans geste de l\'utilisateur, la permission n\'est pas demandée ni l\'option désactivée', async () => {
    const request = vi.fn()
    vi.stubGlobal('Notification', { permission: 'default', requestPermission: request })
    prefs.desktopNotifications = true
    prefs.notifAsked = false
    await setupNotifications()
    expect(request).not.toHaveBeenCalled()
    expect(prefs.desktopNotifications).toBe(true)
    expect(prefs.notifAsked).toBe(false)
    vi.unstubAllGlobals()
  })
})

describe('adresse d\'une notification', () => {
  it('porte le message ciblé', () => {
    expect(parseHash('#/dm/d1/42')).toEqual({ name: 'dm', id: 'd1', messageId: '42' })
    expect(parseHash('#/s/s1/c2/42')).toEqual({ name: 'channel', serverId: 's1', channelId: 'c2', messageId: '42' })
    expect(parseHash('#/s/s1/c2')).toEqual({ name: 'channel', serverId: 's1', channelId: 'c2', messageId: undefined })
  })
})
