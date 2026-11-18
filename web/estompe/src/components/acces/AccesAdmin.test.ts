import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import { compterEffectifs, FILTRES_EFFECTIFS, permissionsModifiees, resumeModifications, type Explication } from '../../lib/acces.svelte'
import AccesAdmin from './AccesAdmin.svelte'

const catalogue = {
  apps: [{
    key: 'estompe', name: 'Estompe', description: '', seen_at: '2026-11-09T10:00:00Z',
    resource_types: [{ app: 'estompe', key: 'server', name: 'Serveur', listable: true }],
    permissions: [
      { name: 'estompe:gifs.use', app: 'estompe', key: 'gifs.use', category: 'Messages', label: 'GIF', description: 'Chercher des GIF', resource_type: null, open_default: true, obsolete_at: null, rules_count: 2 },
      { name: 'estompe:vocal.screen', app: 'estompe', key: 'vocal.screen', category: 'Vocal', label: "Partage d'écran", description: '', resource_type: 'server', open_default: true, obsolete_at: null, rules_count: 1 },
      { name: 'estompe:polls.create', app: 'estompe', key: 'polls.create', category: 'Sondages', label: 'Créer des sondages', description: '', resource_type: null, open_default: false, obsolete_at: null, rules_count: 1 },
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
      { permission: 'estompe:vocal.screen', state: 'partiel', allow: ['*'], deny: ['srv-1'], rules: [
        { rule_id: 'r3', permission: 'estompe:vocal.screen', resource: '*', effect: 'allow', expires_at: null, source: { kind: 'tout_le_monde', role_id: 't' } },
        { rule_id: 'r4', permission: 'estompe:vocal.screen', resource: 'srv-1', effect: 'deny', expires_at: null, source: { kind: 'role', role_id: 'm', role: 'Membre', binding_id: 'b', group_id: null, group: null, scope_type: null, scope_id: null } },
      ] },
      { permission: 'estompe:polls.create', state: 'autorise', allow: ['*'], deny: [], rules: [
        { rule_id: 'r5', permission: 'estompe:polls.create', resource: '*', effect: 'allow', expires_at: null, source: { kind: 'tout_le_monde', role_id: 't' } },
      ] },
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
  if (methode === 'POST' && chemin === '/simulate') return { allowed: false, reason: 'refus explicite', rules: [] } as never
  if (methode === 'PUT' && chemin === '/roles/tlm') return { ...(reponses['/roles/tlm'] as object), rules: (corps as { rules: unknown[] }).rules } as never
  return undefined as never
})

afterEach(() => vi.restoreAllMocks())

describe('AccesAdmin', () => {
  it("affiche les comptes par e-mail et explique leurs droits", async () => {
    const appel = relais()
    const { findByText, findAllByText, getByText, queryByText, getAllByText } = render(AccesAdmin)
    await fireEvent.click(await findByText('alice@plbls.fr'))
    expect(await findByText('Refusé')).toBeTruthy()
    expect(getByText('Partiel')).toBeTruthy()
    expect(await findByText('Les copains')).toBeTruthy()
    expect((await findAllByText(/Tout le monde/)).length).toBeGreaterThan(0)
    expect(getByText(/règle directe/)).toBeTruthy()
    expect(getByText('Ce qui le distingue · 2')).toBeTruthy()
    expect(queryByText('estompe:polls.create')).toBeNull()
    await fireEvent.click(getByText('Toutes · 3'))
    expect(getByText('estompe:polls.create')).toBeTruthy()
    await fireEvent.click(getAllByText('Pourquoi ?')[0])
    expect(await findByText(/Le refus l'emporte toujours/)).toBeTruthy()
    await waitFor(() => expect(appel).toHaveBeenCalledWith('POST', '/simulate', { sub: 'sub-alice', permission: 'estompe:gifs.use', resource: null }))
    expect(await findByText('refus explicite')).toBeTruthy()
  })

  it('range « Tout retirer » dans le menu des autres actions', async () => {
    relais()
    const { findByText, findByLabelText, queryByText } = render(AccesAdmin)
    await fireEvent.click(await findByText('alice@plbls.fr'))
    expect(await findByLabelText('Autres actions')).toBeTruthy()
    expect(queryByText('Tout retirer')).toBeNull()
  })

  it('ouvre « Tout le monde » depuis la grille des rôles, nomme les changements et envoie toutes les règles', async () => {
    const appel = relais()
    const { findByText, getByText, getAllByLabelText, findByLabelText } = render(AccesAdmin)
    await fireEvent.click(await findByText('Rôles', { selector: '[role=tab]' }))
    expect(await findByLabelText('Tout le monde autorise GIF')).toBeTruthy()
    await fireEvent.click(await findByText('Tout le monde'))
    expect(await findByText(/tous les comptes connectés/)).toBeTruthy()
    const refuses = getAllByLabelText('Refuse')
    await fireEvent.click(refuses[1])
    expect(getByText("1 modification non enregistrée · Partage d'écran")).toBeTruthy()
    expect(getByText('modifié')).toBeTruthy()
    await fireEvent.click(getByText('Enregistrer'))
    await waitFor(() => expect(appel).toHaveBeenCalledWith('PUT', '/roles/tlm', expect.objectContaining({
      rules: [
        { permission: 'estompe:gifs.use', resource: '*', effect: 'allow' },
        { permission: 'estompe:vocal.screen', resource: '*', effect: 'deny' },
      ],
    })))
  })
})

describe('droits effectifs', () => {
  const regle = (kind: 'tout_le_monde' | 'compte', effect: 'allow' | 'deny') => ({ rule_id: kind + effect, permission: 'p', resource: '*', effect, expires_at: null, source: kind === 'compte' ? { kind } : { kind, role_id: 't' } })
  const perms = [
    { permission: 'a', state: 'autorise', allow: ['*'], deny: [], rules: [regle('tout_le_monde', 'allow')] },
    { permission: 'b', state: 'refuse', allow: ['*'], deny: ['*'], rules: [regle('tout_le_monde', 'allow'), regle('compte', 'deny')] },
    { permission: 'c', state: 'non_defini', allow: [], deny: [], rules: [] },
  ] as Explication[]

  it('ne garde dans « ce qui le distingue » que ce qui ne vient pas seulement de « Tout le monde »', () => {
    expect(perms.filter(FILTRES_EFFECTIFS.distingue.garde).map((p) => p.permission)).toEqual(['b'])
    expect(compterEffectifs(perms)).toEqual({ distingue: 1, autorise: 1, refuse: 1, tout: 3 })
  })
})

describe("modifications d'un rôle", () => {
  const avant = [{ permission: 'a', resource: '*', effect: 'allow' as const }, { permission: 'b', resource: 'srv', effect: 'deny' as const }]

  it('nomme les permissions ajoutées, retirées ou inversées, pas les autres', () => {
    expect(permissionsModifiees(avant, avant)).toEqual([])
    expect(permissionsModifiees(avant, [{ permission: 'a', resource: '*', effect: 'deny' }, { permission: 'c', resource: '*', effect: 'allow' }])).toEqual(['a', 'b', 'c'])
  })

  it('résume les changements et abrège au-delà de trois', () => {
    expect(resumeModifications([])).toBe('')
    expect(resumeModifications(['Caméra'])).toBe('1 modification non enregistrée · Caméra')
    expect(resumeModifications(['A', 'B', 'C', 'D', 'E'], 1)).toBe('6 modifications non enregistrées · A, B, C et 2 autres')
  })
})
