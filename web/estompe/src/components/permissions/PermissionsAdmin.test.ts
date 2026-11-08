import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import PermissionsAdmin from './PermissionsAdmin.svelte'

const reponses: Record<string, unknown> = {
  '/apps': [{ key: 'estompe', name: 'Estompe', description: '' }, { key: 'permission', name: 'Permission', description: '' }],
  '/apps/estompe/permissions': [{ key: 'access', description: 'Utiliser Estompe' }],
  '/apps/estompe/roles': [{ key: 'membre', name: 'Membre', description: '', permissions: ['access'] }],
  '/assignments': [{ user_id: 'sub-1', app: 'estompe', role: 'membre', granted_by: 'sub-admin', created_at: '2026-11-01T10:00:00Z' }],
}

const relais = () => vi.spyOn(api, 'permissions').mockImplementation(async (methode: string, chemin: string) => (methode === 'GET' ? reponses[chemin] : undefined) as never)

afterEach(() => { vi.restoreAllMocks(); app.rights = [] })

describe('PermissionsAdmin', () => {
  it("passe par le relais de l'API et montre les rôles d'une application", async () => {
    const appel = relais()
    const { findByText, getByText } = render(PermissionsAdmin)
    await fireEvent.click(await findByText('Estompe'))
    expect(await findByText('membre · 1')).toBeTruthy()
    expect(appel).toHaveBeenCalledWith('GET', '/apps/estompe/roles')
    expect(getByText('Rôles', { selector: '[role=tab]' })).toBeTruthy()
  })

  it("protège l'application réservée contre la suppression", async () => {
    relais()
    const { findByText, queryByText } = render(PermissionsAdmin)
    await fireEvent.click(await findByText('Permission'))
    await waitFor(() => expect(queryByText("Supprimer l'application")).toBeNull())
  })

  it('cherche un compte par e-mail seulement avec le droit `users`', async () => {
    relais()
    const sans = render(PermissionsAdmin)
    await fireEvent.click(sans.getByText('Attributions'))
    expect(await sans.findByLabelText('Compte (sub Keycloak)')).toBeTruthy()
    expect(await sans.findByText('sub-1')).toBeTruthy()
    sans.unmount()
    app.rights = ['permissions', 'users']
    const avec = render(PermissionsAdmin)
    await fireEvent.click(avec.getByText('Attributions'))
    expect(await avec.findByLabelText('Compte (e-mail)')).toBeTruthy()
  })
})
