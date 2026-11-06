import { describe, it, expect, vi } from 'vitest'
import { showNotification } from './notify'
import { prefs } from '../../lib/prefs.svelte'

describe('showNotification', () => {
  const n = { title: 'alice', body: 'salut', tag: 'msg-1', url: '/#/dm/d1', conv: 'd:d1', msg: '1', kind: 'dm', silent: true } as const
  it('ouvre la conversation au clic, avec le lien du serveur sans « / » initial', () => {
    const made: { onclick?: () => void; close: () => void }[] = []
    vi.stubGlobal('Notification', Object.assign(function () { const w = { close: () => {} }; made.push(w); return w }, { permission: 'granted' }))
    prefs.desktopNotifications = true
    vi.spyOn(document, 'hasFocus').mockReturnValue(false)
    showNotification(n, false)
    expect(made).toHaveLength(1)
    made[0].onclick!()
    expect(location.hash).toBe('#/dm/d1')
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })
  it('rien quand la conversation est affichée et la fenêtre active', () => {
    const made: unknown[] = []
    vi.stubGlobal('Notification', Object.assign(function () { made.push(1) }, { permission: 'granted' }))
    vi.spyOn(document, 'hasFocus').mockReturnValue(true)
    showNotification(n, true)
    expect(made).toHaveLength(0)
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })
})
