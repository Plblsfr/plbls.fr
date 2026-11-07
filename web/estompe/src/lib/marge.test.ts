import { describe, expect, it } from 'vitest'
import { MARGE_MAX, MARGE_MIN, borneLargeur, lireMarge, marge, ongletEffectif } from './marge.svelte'

describe('marge', () => {
  it('relit l\'état mémorisé et borne la largeur', () => {
    expect(lireMarge(null)).toEqual({ ouverte: true, largeur: 320 })
    expect(lireMarge('pas du json')).toEqual({ ouverte: true, largeur: 320 })
    expect(lireMarge('{"ouverte":false,"largeur":9999}')).toEqual({ ouverte: false, largeur: MARGE_MAX })
    expect(borneLargeur(10)).toBe(MARGE_MIN)
  })

  it('choisit l\'onglet affichable', () => {
    expect(ongletEffectif('membres', true, true)).toBe('membres')
    expect(ongletEffectif('epingles', true, true)).toBe('epingles')
    expect(ongletEffectif('epingles', true, false)).toBe('membres')
    expect(ongletEffectif('membres', false, true)).toBeNull()
    expect(ongletEffectif('epingles', false, true)).toBe('epingles')
  })

  it('les épinglés s\'ouvrent puis rendent l\'état d\'avant', () => {
    marge.large = true
    marge.ouverte = true
    marge.onglet = 'membres'
    marge.basculerEpingles()
    expect(marge.visible && marge.onglet).toBe('epingles')
    marge.basculerEpingles()
    expect(marge.visible && marge.onglet).toBe('membres')
    marge.ouverte = false
    marge.basculerEpingles()
    expect(marge.visible).toBe(true)
    marge.basculerEpingles()
    expect(marge.visible).toBe(false)
  })

  it('sous 1200 px la marge se superpose sans toucher à l\'état mémorisé', () => {
    marge.large = false
    marge.ouverte = true
    marge.superposee = false
    expect(marge.visible).toBe(false)
    marge.basculer()
    expect(marge.visible).toBe(true)
    expect(marge.ouverte).toBe(true)
  })
})
