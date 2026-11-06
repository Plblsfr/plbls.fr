import { describe, expect, it } from 'vitest'
import { platform } from './platform'

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
})
