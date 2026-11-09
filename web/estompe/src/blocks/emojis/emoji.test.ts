import { beforeAll, describe, expect, it, vi } from 'vitest'
import { EMOJI_GROUPS, codeAt, completeCode, emojiByCode, loadEmojiRows, loadRecents, pushRecent, reactionTitle, searchEmojis, type EmojiRow } from './emoji'

let data: EmojiRow[]
beforeAll(async () => { data = await loadEmojiRows() })

describe('conversion des codes :nom:', () => {
  const find = (n: string) => (n === 'perso' ? undefined : emojiByCode(n))
  const type = (t: string) => completeCode(t, t.length, find)

  it('remplace un code complet dès le second « : » et place le curseur après', () => {
    expect(type('Ajouter la conversion de :joy:')).toEqual({ text: 'Ajouter la conversion de 😂', caret: 'Ajouter la conversion de 😂'.length })
    expect(type(':thumbsup:')?.text).toBe('👍')
  })
  it('laisse un code inconnu ou réservé tel quel', () => {
    expect(type('salut :zzzzzzz:')).toBeNull()
    expect(type('salut :perso:')).toBeNull()
  })
  it('ne convertit ni dans un bloc de code, ni dans du code en ligne, ni dans une URL, ni collé à un mot', () => {
    expect(type('```\nx :joy:')).toBeNull()
    expect(type('du `code :joy:')).toBeNull()
    expect(type('http://exemple.fr:joy:')).toBeNull()
    expect(type('à 12:30:45')).toBeNull()
    expect(type('a:joy:')).toBeNull()
    expect(type('```\ncode\n``` puis :joy:')?.text).toBe('```\ncode\n``` puis 😂')
  })
})

describe('autocomplétion', () => {
  it('propose à partir de « : » suivi de deux caractères', () => {
    expect(codeAt('bonjour :jo', 11)).toEqual({ start: 8, query: 'jo' })
    expect(codeAt('bonjour :j', 10)).toBeNull()
    expect(codeAt('http://exemple.fr:jo', 20)).toBeNull()
    expect(codeAt('```\n:jo', 7)).toBeNull()
    expect(searchEmojis(data, 'jo').slice(0, 5).map((r) => r[1])).toContain('joy')
  })
})

describe('recherche', () => {
  const first = (q: string) => searchEmojis(data, q)[0]?.[0]
  it('trouve par code, avec ou sans « : », par nom français et par mot-clé anglais ou français', () => {
    expect(first(':joy:')).toBe('😂')
    expect(first('joy')).toBe('😂')
    expect(first('pouce vers le haut')).toBe('👍')
    expect(searchEmojis(data, 'thumbs').map((r) => r[0])).toContain('👍')
    expect(searchEmojis(data, 'fromage').map((r) => r[0])).toContain('🧀')
    expect(searchEmojis(data, 'zzzzzzzzz')).toEqual([])
  })
  it('classe les neuf catégories et ne laisse aucun smiley hors catégorie', () => {
    expect(new Set(data.map((r) => r[5]))).toEqual(new Set(EMOJI_GROUPS.keys()))
  })
})

describe('récents', () => {
  it('place le dernier utilisé en tête, sans doublon', () => {
    localStorage.clear()
    pushRecent('😂')
    pushRecent('👍')
    expect(pushRecent('😂')).toEqual(['😂', '👍'])
    expect(loadRecents()).toEqual(['😂', '👍'])
  })
})

const u = (id: string) => ({ id, pseudo: id })
describe('reactionTitle', () => {
  it('liste les personnes, « Toi » pour soi, et regroupe au-delà du maximum', () => {
    expect(reactionTitle({ emoji: '😂', users: [u('bob')] }, 'me')).toBe('bob a réagi avec 😂')
    expect(reactionTitle({ emoji: '😂', users: [u('bob'), u('me')] }, 'me')).toBe('bob et Toi ont réagi avec 😂')
    expect(reactionTitle({ emoji: '👍', users: [u('a'), u('b'), u('c'), u('d'), u('e')] }, 'me')).toBe('a, b, c et 2 autres ont réagi avec 👍')
  })
})

describe('chargement à la demande', () => {
  it('réessaie au prochain besoin après un échec, sans garder la promesse rejetée', async () => {
    vi.resetModules()
    let essais = 0
    vi.doMock('./emoji-data.json', () => {
      essais++
      if (essais === 1) throw new Error('hors ligne')
      return { default: [['😂', 'joy', ['lol'], [], 'visage qui pleure de rire', 0]] }
    })
    const m = await import('./emoji')
    await expect(m.loadEmojiRows()).rejects.toThrow()
    expect(m.emojiByCode('joy')).toBeUndefined()
    const rows = await m.loadEmojiRows()
    expect(rows).toHaveLength(1)
    expect(m.emojiByCode('lol')).toBe('😂')
    expect(essais).toBe(2)
    vi.doUnmock('./emoji-data.json')
    vi.resetModules()
  })
})
