import { describe, it, expect } from 'vitest'
import { BLOCKS, blockOn } from './registry'

describe('blockOn', () => {
  it('propose tout sans liste du serveur', () => {
    expect(BLOCKS.every((b) => blockOn(undefined, b.name))).toBe(true)
  })
  it('masque un bloc absent de la liste', () => {
    expect(blockOn(['vocal'], 'gifs')).toBe(false)
    expect(blockOn(['gifs'], 'gifs')).toBe(true)
  })
  it('masque un bloc dont une dépendance est inactive', () => {
    expect(blockOn(['regarder-ensemble'], 'regarder-ensemble')).toBe(false)
    expect(blockOn(['regarder-ensemble', 'vocal'], 'regarder-ensemble')).toBe(true)
  })
  it('films dépend du vocal et de regarder-ensemble', () => {
    expect(blockOn(['films', 'vocal'], 'films')).toBe(false)
    expect(blockOn(['films', 'regarder-ensemble', 'vocal'], 'films')).toBe(true)
  })
  it('ignore un bloc inconnu', () => {
    expect(blockOn(['inconnu'], 'inconnu')).toBe(false)
  })
})

describe('dossiers de blocs', () => {
  const dirs = [...new Set(Object.keys(import.meta.glob('./*/*')).map((f) => f.split('/')[1]))]
  it('chaque dossier est un bloc déclaré', () => {
    expect(dirs.filter((d) => !BLOCKS.some((b) => b.name === d))).toEqual([])
  })
})
