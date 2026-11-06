import { describe, expect, it } from 'vitest'
import { groupesCites, insertMention, mentionAt, suggest } from './mention'

describe('mentionAt', () => {
  it('détecte une mention en cours', () => {
    expect(mentionAt('salut @al', 9)).toEqual({ start: 6, query: 'al' })
    expect(mentionAt('@', 1)).toEqual({ start: 0, query: '' })
    expect(mentionAt('a\n@bo', 5)).toEqual({ start: 2, query: 'bo' })
  })
  it('ignore les adresses e-mail et les mentions déjà terminées', () => {
    expect(mentionAt('mail@alice', 10)).toBeNull()
    expect(mentionAt('@alice ', 7)).toBeNull()
    expect(mentionAt('rien', 4)).toBeNull()
  })
  it('regarde seulement avant le curseur', () => {
    expect(mentionAt('@al suite', 3)).toEqual({ start: 0, query: 'al' })
  })
})

describe('insertMention', () => {
  it('remplace la saisie et place le curseur après l\'espace', () => {
    expect(insertMention('salut @al ok', 9, 6, 'alice')).toEqual({ text: 'salut @alice  ok', caret: 13 })
  })
})

describe('suggest', () => {
  const all = ['Alice', 'bob', 'Malik', 'moi']
  it('classe les débuts avant les contenus et exclut soi-même', () => {
    expect(suggest('al', all, 'moi')).toEqual(['Alice', 'Malik'])
    expect(suggest('', all, 'moi')).toEqual(['Alice', 'bob', 'Malik'])
    expect(suggest('zzz', all, 'moi')).toEqual([])
  })
  it('limite le nombre de propositions', () => {
    expect(suggest('', Array.from({ length: 20 }, (_, i) => `user${i}`), 'moi', 3)).toHaveLength(3)
  })
})

describe('groupesCites', () => {
  it('trouve @everyone et @here hors code', () => {
    expect(groupesCites('@here et @Everyone')).toEqual(['here', 'everyone'])
    expect(groupesCites('`@everyone` et ```\n@here\n```')).toEqual([])
    expect(groupesCites('mail@here.fr @heres')).toEqual([])
  })
})
