import { describe, expect, it } from 'vitest'
import { searchSettings, SETTINGS_TABS } from './settings.svelte'

describe('recherche dans les paramètres', () => {
  it('montre tous les onglets sans recherche', () => {
    expect(searchSettings('  ').map((t) => t.id)).toEqual(SETTINGS_TABS.map((t) => t.id))
  })
  it('trouve un onglet par son nom ou par un réglage, sans tenir compte des accents ni de la casse', () => {
    expect(searchSettings('debogage').map((t) => t.id)).toEqual(['debug'])
    const r = searchSettings('THEME')
    expect(r.map((t) => t.id)).toEqual(['appearance'])
    expect(r[0].hits).toEqual(['Thème'])
    expect(searchSettings('ne pas déranger').map((t) => t.id)).toEqual(['notifications'])
  })
  it('liste les onglets de chaque réglage trouvé', () => {
    expect(searchSettings('camera').map((t) => t.id)).toEqual(['voice'])
    expect(searchSettings('micro').find((t) => t.id === 'voice')?.hits).toContain('Volume du micro')
  })
  it('ne renvoie rien quand rien ne correspond', () => {
    expect(searchSettings('zzz')).toEqual([])
  })
})
