import { beforeEach, describe, expect, it } from 'vitest'
import { clearRecentSearches, excerpt, highlightTerms, recentSearches, rememberSearch, suggestions } from './search'

const ctx = { pseudos: ['Alice', 'Bob'], channels: ['général', 'jeux'], servers: ['Mon serveur'] }

describe('suggestions', () => {
  it('propose les clés de filtre, sans tenir compte des accents', () => {
    expect(suggestions('bonjour ap', ctx)).toEqual([{ label: 'après: AAAA-MM-JJ', insert: 'bonjour après:' }])
    expect(suggestions('epi', ctx).map((s) => s.insert)).toEqual(['épinglé:'])
  })
  it('propose les valeurs du filtre en cours', () => {
    expect(suggestions('truc de:al', ctx)).toEqual([{ label: 'Alice', insert: 'truc de:Alice ' }])
    expect(suggestions('dans:g', ctx)[0].insert).toBe('dans:#général ')
    expect(suggestions('serveur:mon', ctx)[0].insert).toBe('serveur:"Mon serveur" ')
    expect(suggestions('a:', ctx).map((s) => s.label)).toEqual(['fichier', 'image', 'vidéo', 'lien', 'sondage'])
  })
  it('ne propose rien pour une valeur complète, un mot exclu ou entre guillemets', () => {
    expect(suggestions('de:alice', ctx)).toEqual([])
    expect(suggestions('-de', ctx)).toEqual([])
    expect(suggestions('"de', ctx)).toEqual([])
    expect(suggestions('', ctx)).toEqual([])
  })
})

describe('surlignage', () => {
  it('ignore filtres, exclusions et guillemets, et garde les racines', () => {
    expect(highlightTerms('"Réunions" -demain de:alice a:image salut')).toEqual(['reunion', 'salut'])
  })
  it('marque les termes sans tenir compte des accents ni de la casse', () => {
    expect(excerpt('Une RÉUNION importante', ['reunion'])).toEqual([
      { text: 'Une ', hit: false }, { text: 'RÉUNION', hit: true }, { text: ' importante', hit: false },
    ])
  })
  it('centre un long message sur le premier terme', () => {
    const p = excerpt('x'.repeat(300) + ' cible ' + 'y'.repeat(300), ['cible'], 100)
    expect(p[0].text).toBe('…')
    expect(p.find((x) => x.hit)?.text).toBe('cible')
    expect(p.at(-1)?.text).toBe('…')
    expect(p.map((x) => x.text).join('').length).toBeLessThan(110)
  })
})

describe('dernières recherches', () => {
  beforeEach(() => localStorage.clear())
  it('garde les plus récentes sans doublon, 10 au plus, et s’efface', () => {
    for (let i = 0; i < 12; i++) rememberSearch('q' + i)
    rememberSearch('q5')
    const l = recentSearches()
    expect(l).toHaveLength(10)
    expect(l[0]).toBe('q5')
    expect(new Set(l).size).toBe(10)
    clearRecentSearches()
    expect(recentSearches()).toEqual([])
  })
})
