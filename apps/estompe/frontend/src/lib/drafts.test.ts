import { beforeEach, describe, expect, it } from 'vitest'
import { clearDraft, draftKeys, keepSession, markDraft, persistFiles, sessionFiles, MAX_DRAFT_BYTES } from './drafts.svelte'

describe('brouillons', () => {
  beforeEach(() => { draftKeys.clear(); clearDraft('c:1'); clearDraft('c:2') })

  it('garde les pièces jointes de chaque salon pendant la session', () => {
    const a = [{ key: 1, name: 'a.png', size: 3, id: 'f1' }]
    keepSession('c:1', a)
    keepSession('c:2', [])
    expect(sessionFiles('c:1')).toBe(a)
    expect(sessionFiles('c:2')).toEqual([])
    expect(sessionFiles('c:3')).toBeUndefined()
  })

  it("oublie le brouillon à l'envoi", () => {
    keepSession('c:1', [{ key: 1, name: 'a.png', size: 3, id: 'f1' }])
    clearDraft('c:1')
    expect(sessionFiles('c:1')).toBeUndefined()
  })

  it('signale les salons ayant un brouillon', () => {
    markDraft('c:1', true)
    markDraft('c:2', true)
    markDraft('c:2', false)
    expect([...draftKeys]).toEqual(['c:1'])
  })

  it('refuse de conserver des fichiers trop lourds après un redémarrage', async () => {
    const big = { size: MAX_DRAFT_BYTES + 1 } as File
    expect(await persistFiles('c:1', [big])).toBe(false)
  })
})
