import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import AccesAdmin from './AccesAdmin.svelte'

const catalogue = {
  apps: [{
    key: 'estompe', name: 'Estompe', description: '', seen_at: '2026-11-09T10:00:00Z',
    resource_types: [{ app: 'estompe', key: 'server', name: 'Serveur', listable: true }],
    permissions: [
      { name: 'estompe:gifs.use', app: 'estompe', key: 'gifs.use', category: 'Messages', label: 'GIF', description: 'Chercher des GIF', resource_type: null, open_default: true, obsolete_at: null, rules_count: 2 },
      { name: 'estompe:vocal.screen', app: 'estompe', key: 'vocal.screen', category: 'Vocal', label: "Partage d'écran", description: '', resource_type: 'server', open_default: true, obsolete_at: null, rules_count: 1 },
    ],
  }],
}
const alice = { sub: 'sub-alice', email: 'alice@plbls.fr', display_name: 'Alice', last_seen_at: '2026-11-09T09:00:00Z', in_keycloak: true }
const reponses: Record<string, unknown> = {
  '/catalog': catalogue,
  '/accounts?limit=100': { accounts: [{ ...alice, roles_count: 1, groups_count: 0, allows_count: 0, denies_count: 1 }], total: 1, from_keycloak: 0 },
  '/accounts/sub-alice': { account: alice, bindings: [], groups: [], rules: [], group_bindings: [] },
  '/accounts/sub-alice/effective': {
    account: alice,
    permissions: [
      { permission: 'estompe:gifs.use', state: 'refuse', allow: ['*'], deny: ['*'], rules: [
        { rule_id: 'r1', permission: 'estompe:gifs.use', resource: '*', effect: 'allow', expires_at: null, source: { kind: 'tout_le_monde', role_id: 't' } },
        { rule_id: 'r2', permission: 'estompe:gifs.use', resource: '*', effect: 'deny', expires_at: null, source: { kind: 'compte' } },
      ] },
      { permission: 'estompe:vocal.screen', state: 'partiel', allow: ['*'], deny: ['srv-1'], rules: [] },
    ],
  },
  '/roles': [{ id: 'tlm', name: 'Tout le monde', description: '', system: 'tout-le-monde', created_at: '', rules_count: 2, bindings_count: 0, accounts_count: 3 }],
  '/roles/tlm': { id: 'tlm', name: 'Tout le monde', description: '', system: 'tout-le-monde', created_at: '', rules_count: 1, bindings_count: 0, accounts_count: 3, rules: [
    { id: 'x', subject_kind: 'role', subject_id: 'tlm', permission: 'estompe:gifs.use', resource: '*', effect: 'allow', expires_at: null, note: '', granted_by: 'manifeste:estompe', created_at: '', obsolete: false },
  ], bindings: [] },
}

const relais = () => vi.spyOn(api, 'acces').mockImplementation(async (methode: string, chemin: string, corps?: unknown) => {
  if (methode === 'GET' && chemin.startsWith('/resources/')) return [{ id: 'srv-1', name: 'Les copains' }] as never
  if (methode === 'GET') return reponses[chemin] as never
  if (methode === 'PUT' && chemin === '/roles/tlm') return { ...(reponses['/roles/tlm'] as object), rules: (corps as { rules: unknown[] }).rules } as never
  return undefined as never
})

afterEach(() => vi.restoreAllMocks())

describe('AccesAdmin', () => {
  it("affiche les comptes par e-mail et explique leurs droits", async () => {
    relais()
    const { findByText, findAllByText, getByText } = render(AccesAdmin)
    await fireEvent.click(await findByText('alice@plbls.fr'))
    expect(await findByText('Refusé')).toBeTruthy()
    expect(getByText('Partiel')).toBeTruthy()
    expect(await findByText('Les copains')).toBeTruthy()
    expect((await findAllByText(/Tout le monde/)).length).toBeGreaterThan(0)
    expect(getByText(/règle directe/)).toBeTruthy()
  })

  it('édite « Tout le monde » en grille et envoie toutes les règles', async () => {
    const appel = relais()
    const { findByText, getByText, getAllByLabelText } = render(AccesAdmin)
    await fireEvent.click(await findByText('Rôles', { selector: '[role=tab]' }))
    await fireEvent.click(await findByText('Tout le monde'))
    expect(await findByText(/tous les comptes connectés/)).toBeTruthy()
    const refuses = getAllByLabelText('Refusé')
    await fireEvent.click(refuses[1])
    await fireEvent.click(getByText('Enregistrer'))
    await waitFor(() => expect(appel).toHaveBeenCalledWith('PUT', '/roles/tlm', expect.objectContaining({
      rules: [
        { permission: 'estompe:gifs.use', resource: '*', effect: 'allow' },
        { permission: 'estompe:vocal.screen', resource: '*', effect: 'deny' },
      ],
    })))
  })
})
