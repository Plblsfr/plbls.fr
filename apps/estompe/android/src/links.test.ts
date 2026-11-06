import { describe, it, expect, vi } from 'vitest'
import { linkAction, openFromNotification } from './links'

describe('openFromNotification', () => {
  const open = (url: string) => {
    vi.stubGlobal('window', { EstompeNative: { takeOpenUrl: () => url } })
    return openFromNotification()
  }
  it('ouvre la conversation de la notification', () => {
    location.hash = ''
    expect(open('/#/dm/d1')).toBe(true)
    expect(location.hash).toBe('#/dm/d1')
    vi.unstubAllGlobals()
  })
  it('ignore une adresse qui n\'est pas interne', () => {
    location.hash = ''
    expect(open('https://exemple.fr/')).toBe(false)
    expect(location.hash).toBe('')
    vi.unstubAllGlobals()
  })
})


describe('linkAction', () => {
  it('ouvre un lien externe, mailto et tel dans le système', () => {
    expect(linkAction('https://example.org/a?b=1')).toEqual({ open: 'https://example.org/a?b=1' })
    expect(linkAction('mailto:a@b.fr')).toEqual({ open: 'mailto:a@b.fr' })
    expect(linkAction('tel:+33123456789')).toEqual({ open: 'tel:+33123456789' })
  })
  it('route un lien interne vers la vue de l\'app', () => {
    expect(linkAction('https://estompe.plbls.fr/#/dm/42')).toEqual({ hash: '#/dm/42' })
  })
  it('route les liens de l\'interface (origine de la webview, ex. tauri.localhost) sans les ouvrir dehors', () => {
    expect(linkAction(`${location.origin}/#/s/1/c/2`)).toEqual({ hash: '#/s/1/c/2' })
  })
  it('refuse les schémas dangereux', () => {
    for (const h of ['javascript:alert(1)', 'intent://x#Intent;end', 'file:///etc/passwd', 'nimporte quoi']) expect(linkAction(h)).toBeNull()
  })
})
