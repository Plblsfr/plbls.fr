import { describe, expect, it } from 'vitest'
import { formatReport, loadDraft, saveDraft } from './report'

describe('brouillon de signalement', () => {
  it('se conserve puis se vide', () => {
    expect(loadDraft()).toEqual({ title: '', description: '', category: '' })
    saveDraft({ title: 'Le vocal coupe', description: 'après 5 min', category: 'vocal' })
    expect(loadDraft().title).toBe('Le vocal coupe')
    saveDraft({ title: '', description: '', category: '' })
    expect(loadDraft().description).toBe('')
  })
})

describe('formatReport', () => {
  it('reprend titre, informations techniques et logs', () => {
    const t = formatReport({ title: 'Bug', author: 'alice', created_at: '2026-10-26T10:00:00Z', category: 'vocal', platform: 'web', status: 'new', description: 'détail', tech: { Version: '1.0' }, logs: 'ligne' })
    expect(t).toContain('# Bug')
    expect(t).toContain('Vocal · Web · Nouveau')
    expect(t).toContain('- Version : 1.0')
    expect(t).toContain('## Logs\nligne')
  })
})
