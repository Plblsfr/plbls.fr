import { afterEach, describe, expect, it, vi } from 'vitest'
import { reseauDuNavigateur } from './reseau'

describe('type de connexion annoncé par le navigateur', () => {
  afterEach(() => { Reflect.deleteProperty(navigator, 'connection') })
  const connexion = (c: unknown) => Object.defineProperty(navigator, 'connection', { configurable: true, value: c })

  it('reste inconnu quand le navigateur ne dit rien', () => {
    expect(reseauDuNavigateur.kind()).toBe('unknown')
    connexion({ effectiveType: '4g' })
    expect(reseauDuNavigateur.kind()).toBe('unknown')
    expect(reseauDuNavigateur.watch(() => {})()).toBeUndefined()
  })

  it('distingue le Wi-Fi des données mobiles et suit les changements', () => {
    const c = { type: 'cellular', addEventListener: vi.fn(), removeEventListener: vi.fn() }
    connexion(c)
    expect(reseauDuNavigateur.kind()).toBe('cellular')
    c.type = 'wifi'
    expect(reseauDuNavigateur.kind()).toBe('wifi')
    c.type = 'ethernet'
    expect(reseauDuNavigateur.kind()).toBe('wifi')
    c.type = 'bluetooth'
    expect(reseauDuNavigateur.kind()).toBe('unknown')
    const suivi = () => {}
    const fin = reseauDuNavigateur.watch(suivi)
    expect(c.addEventListener).toHaveBeenCalledWith('change', suivi)
    fin()
    expect(c.removeEventListener).toHaveBeenCalledWith('change', suivi)
  })
})
