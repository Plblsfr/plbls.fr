import { describe, expect, it, vi } from 'vitest'
import { platform } from './platform'
import { installeur, partagerImage } from './browser'

describe('plateforme web', () => {
  it('se déclare « web » dans un onglet, « pwa » une fois installée', () => {
    expect(platform.id()).toBe('web')
    window.matchMedia = ((q: string) => ({ matches: q.includes('standalone') })) as typeof window.matchMedia
    expect(platform.id()).toBe('pwa')
    expect(platform.restoresLastView()).toBe(true)
  })
  it('n\'a ni connexion native, ni partage natif, ni espace dédié', () => {
    expect(platform.auth).toBeUndefined()
    expect(platform.capture).toBeUndefined()
    expect(platform.spaces).toEqual([])
    expect(platform.updates.installOnBlock).toBe(true)
  })
  it("donne une adresse d'installeur propre à chaque version, qu'aucun cache n'a pu garder", () => {
    expect(installeur('Estompe-setup.exe')).toBe(`/updates/Estompe-setup.exe?v=${__APP_VERSION__}`)
    expect(installeur('Estompe.apk')).toMatch(/^\/updates\/Estompe\.apk\?v=\d+\.\d+\.\d+$/)
  })
  it('partage une image par la feuille du système, ou dit qu\'il ne peut pas', async () => {
    const image = new Blob(['x'], { type: 'image/png' })
    const nav = navigator as unknown as { canShare?: unknown; share?: unknown }
    nav.canShare = undefined
    expect(await partagerImage(image, 'b.png', 't')).toBe(false)
    const share = vi.fn(async () => {})
    nav.canShare = () => true
    nav.share = share
    expect(await partagerImage(image, 'b.png', 'Film terminé')).toBe(true)
    expect(share).toHaveBeenCalledWith({ files: [expect.any(File)], text: 'Film terminé' })
    nav.share = vi.fn(async () => { throw new DOMException('non', 'AbortError') })
    expect(await partagerImage(image, 'b.png', 't')).toBe(true)
    nav.share = vi.fn(async () => { throw new DOMException('non', 'NotAllowedError') })
    expect(await partagerImage(image, 'b.png', 't')).toBe(false)
    nav.canShare = () => false
    expect(await partagerImage(image, 'b.png', 't')).toBe(false)
  })
})
