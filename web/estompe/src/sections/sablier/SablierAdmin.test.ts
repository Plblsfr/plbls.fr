import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ConfirmModal from '../../components/ConfirmModal.svelte'
import { ApiError } from '../../lib/api'
import { confirmation } from '../../lib/confirm.svelte'
import { router } from '../../lib/route.svelte'
import { app } from '../../lib/store.svelte'
import { admin } from './admin.svelte'
import { AUJOURDHUI, catalogue, fauxClient, journal, personnes, statsPleines, tropheesAdmin, trophees } from './essais'
import SablierAdmin from './SablierAdmin.svelte'
import SablierSpace from './SablierSpace.svelte'
import { adminApi, sablierApi, type ClientSablier, type SablierMe } from './sablier'
import { sablier } from './sablier.svelte'

const moi = (adminOuvert: boolean): SablierMe => ({ sub: 'kc-moi', name: 'moi', email: '', permissions: { access: true, admin: adminOuvert }, sections: ['stats'] })
const ORPHELIN = '7f3a9d2e-0000-4000-8000-0000000000c2'
let appels: string[]

beforeEach(() => {
  appels = []
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date(`${AUJOURDHUI}T10:00:00`))
  sablier.reinitialiser()
  admin.reinitialiser()
  app.sablierMe = moi(true)
  const client = fauxClient()
  for (const cle of Object.keys(client) as (keyof ClientSablier)[]) vi.spyOn(sablierApi, cle).mockImplementation(client[cle] as never)
  vi.spyOn(adminApi, 'personnes').mockImplementation(async () => personnes())
  vi.spyOn(adminApi, 'catalogue').mockImplementation(async () => catalogue())
  vi.spyOn(adminApi, 'trophees').mockImplementation(async () => tropheesAdmin())
  vi.stubGlobal('fetch', vi.fn(async (url: string, init: RequestInit) => {
    appels.push(`${init.method} ${url}`)
    const chemin = url.replace(/\?.*$/, '')
    const s = statsPleines()
    const corps = chemin.endsWith('/usage') ? s.usage : chemin.endsWith('/entracte') ? s.entracte : chemin.endsWith('/sillon') ? s.sillon : chemin.endsWith('/messaging') ? s.messaging : chemin.endsWith('/trophies') ? trophees() : chemin.endsWith('/streaks') ? { global: { current: 12, best: 41 }, entracte: { current: 0, best: 9 }, sillon: { current: 3, best: 3 }, messaging: { current: 5, best: 41 } } : []
    return new Response(JSON.stringify(corps), { status: 200, headers: { 'Content-Type': 'application/json' } })
  }))
})
afterEach(() => { confirmation.en?.resoudre({ ok: false, valeur: '' }); vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); sablier.reinitialiser(); admin.reinitialiser(); app.sablierMe = null; app.details = {}; history.replaceState(null, '', '#/') })

describe('administration des stats', () => {
  it('C1 : sans sablier:admin, la route d’administration retombe sur le jeu et les trophées', async () => {
    app.sablierMe = moi(false)
    const lister = vi.mocked(adminApi.personnes)
    const vue = render(SablierSpace, { section: 'admin', id: 'personnes', detail: 'kc-camille', onmenu: () => {} })
    expect(await vue.findByRole('region', { name: 'Stats par domaine' })).toBeInTheDocument()
    expect(vue.queryByRole('heading', { name: 'Administration des stats' })).toBeNull()
    expect(vue.queryByRole('navigation', { name: 'Administration' })).toBeNull()
    expect(lister).not.toHaveBeenCalled()
    expect(appels).toEqual([])
  })

  it('C1 : avec sablier:admin, les onglets et la liste des personnes', async () => {
    const vue = render(SablierSpace, { section: 'admin', onmenu: () => {} })
    expect(vue.getByRole('heading', { level: 1, name: 'Administration des stats' })).toBeInTheDocument()
    const onglets = within(vue.getByRole('navigation', { name: 'Administration' })).getAllByRole('link')
    expect(onglets.map((a) => [a.textContent, a.getAttribute('href'), a.getAttribute('aria-current')])).toEqual([['Personnes', '#/stats/admin/personnes', 'page'], ['Totaux', '#/stats/admin/totaux', null], ['Journal', '#/stats/admin/journal', null], ['Trophées', '#/stats/admin/trophees', null]])
    const liste = within(await vue.findByRole('region', { name: 'Personnes' }))
    expect(liste.getByText(/est noté au journal/)).toBeInTheDocument()
    expect(liste.getAllByRole('rowheader').map((th) => th.textContent)).toEqual(['Camille', 'moitoi'])
    expect(liste.getByRole('link', { name: 'Voir ses stats, Camille' })).toHaveAttribute('href', '#/stats/admin/personnes/kc-camille')
    await fireEvent.input(liste.getByLabelText('Chercher une personne'), { target: { value: 'cam' } })
    expect(liste.getAllByRole('rowheader').map((th) => th.textContent)).toEqual(['Camille'])
  })

  it('C6 : les personnes sans compte sont listées, l’effacement passe par la même confirmation EFFACER', async () => {
    const effacer = vi.spyOn(adminApi, 'effacer').mockResolvedValue(undefined)
    render(ConfirmModal)
    const vue = render(SablierAdmin, { onglet: 'personnes' })
    const sans = within(await vue.findByRole('region', { name: 'Personnes sans compte' }))
    expect(sans.getByText('Compte supprimé · 7f3a…c2')).toBeInTheDocument()
    expect(sans.getByText(/Stats du 3 mars 2024 au 12 juin 2026 · dernière activité il y a 5 mois/)).toBeInTheDocument()
    expect(sans.queryByText(/cette liste est approximative/)).toBeNull()
    await fireEvent.click(sans.getByRole('button', { name: 'Effacer ses données…, Compte supprimé · 7f3a…c2' }))
    const boite = within(await waitFor(() => { const d = document.querySelector<HTMLElement>('[role="alertdialog"]'); expect(d).not.toBeNull(); return d! }))
    expect(boite.getByText(/Un import d’historique rejoué ensuite serait compté de nouveau/)).toBeInTheDocument()
    const valider = boite.getByRole('button', { name: 'Effacer définitivement' })
    expect(valider).toBeDisabled()
    await fireEvent.input(boite.getByLabelText('Écris EFFACER pour confirmer'), { target: { value: 'effacer' } })
    expect(valider).toBeDisabled()
    await fireEvent.input(boite.getByLabelText('Écris EFFACER pour confirmer'), { target: { value: 'EFFACER' } })
    await fireEvent.click(valider)
    await waitFor(() => expect(effacer).toHaveBeenCalledWith(ORPHELIN))
    await waitFor(() => expect(vi.mocked(adminApi.personnes)).toHaveBeenCalledTimes(2))
  })

  it('annuaire local : l’écran prévient que la liste des personnes sans compte est approximative', async () => {
    vi.mocked(adminApi.personnes).mockResolvedValue({ directory: 'local', people: personnes().people.filter((p) => p.known) })
    const vue = render(SablierAdmin, { onglet: 'personnes' })
    const sans = within(await vue.findByRole('region', { name: 'Personnes sans compte' }))
    expect(sans.getByRole('note')).toHaveTextContent('cette liste est approximative')
    expect(sans.getByText('Aucune personne sans compte.')).toBeInTheDocument()
  })

  it('stats d’une personne : routes d’administration, bandeau « notée au journal », rien n’est marqué vu', async () => {
    app.details = { s1: { server: { id: 's1', name: 'Amis', owner_id: 'me', icon_version: 0 }, channels: [{ id: 'c1', name: 'général', kind: 'text', position: 0 }], members: [] } }
    const vue = render(SablierAdmin, { onglet: 'personnes', detail: 'kc-camille' })
    const page = within(await vue.findByRole('region', { name: 'Stats de Camille' }))
    expect((await page.findByText(/^Consultation notée au journal/)).textContent).toBe('Consultation notée au journal le 18 nov. 2026 à 10:00. Camille la voit dans « Partage et données ».')
    expect(page.getByText('31 h 20')).toBeInTheDocument()
    expect(page.getByText('# général')).toBeInTheDocument()
    expect(page.getByText('salon supprimé ou inaccessible')).toBeInTheDocument()
    expect(page.getByRole('button', { name: 'Effacer ses données…' })).toBeInTheDocument()
    const lus = appels.map((a) => a.replace(/\?.*$/, ''))
    for (const route of ['usage', 'entracte', 'sillon', 'messaging', 'trophies', 'streaks', 'recaps']) expect(lus).toContain(`GET /admin/people/kc-camille/${route}`)
    expect(appels.every((a) => a.startsWith('GET /admin/people/kc-camille/'))).toBe(true)
    expect(sablierApi.marquerVus).not.toHaveBeenCalled()
    expect(sablierApi.nonVus).not.toHaveBeenCalled()
    expect(document.querySelector('[data-annonce]')).toBeNull()
    await fireEvent.click(page.getByRole('button', { name: 'Année' }))
    await waitFor(() => expect(appels.some((a) => a.startsWith('GET /admin/people/kc-camille/usage?period=year'))).toBe(true))
  })

  it('ses propres stats vues par l’administration : pas de bandeau « notée au journal »', async () => {
    const vue = render(SablierAdmin, { onglet: 'personnes', detail: 'kc-moi' })
    const page = within(await vue.findByRole('region', { name: 'Stats de moi' }))
    expect(await page.findByRole('note')).toHaveTextContent('cette consultation n’est pas notée au journal')
    expect(page.queryByText(/^Consultation notée au journal/)).toBeNull()
  })

  it('totaux : les tuiles non nulles, rechargées au changement de période', async () => {
    const totaux = vi.spyOn(adminApi, 'totaux').mockImplementation(async (p) => ({ period: p, from: '2026-11-01', to: '2026-11-30', active_people: 14, usage_seconds: 388 * 3600, watched_seconds: 0, listened_seconds: 164 * 3600, messages: 6420, by_day: [] }))
    const vue = render(SablierAdmin, { onglet: 'totaux' })
    const page = within(await vue.findByRole('region', { name: 'Totaux globaux' }))
    expect(await page.findByText('388 h')).toBeInTheDocument()
    expect(page.getByText(/^6.420$/)).toBeInTheDocument()
    expect(page.queryByText('Temps regardé')).toBeNull()
    await fireEvent.click(page.getByRole('button', { name: 'Semaine' }))
    await waitFor(() => expect(totaux).toHaveBeenCalledWith('week', AUJOURDHUI))
  })

  it('journal : date, admin, action et personne, puis les lignes plus anciennes', async () => {
    const lire = vi.spyOn(adminApi, 'journal').mockImplementation(async (avant) => (avant ? { directory: 'aegis', entries: journal().entries.slice(2), next_before: null } : { directory: 'aegis', entries: journal().entries.slice(0, 2), next_before: 2 }))
    const vue = render(SablierAdmin, { onglet: 'journal' })
    const page = within(await vue.findByRole('region', { name: 'Journal des consultations' }))
    const lignes = () => page.getAllByRole('row').slice(1).map((tr) => [...tr.children].map((c) => c.textContent).slice(1).join(' | '))
    await waitFor(() => expect(lignes()).toEqual(['Sacha | Consultation | Camille', 'Inès | Effacement des données | Sans nom · 7f3a…c2']))
    await fireEvent.click(page.getByRole('button', { name: 'Plus anciens' }))
    await waitFor(() => expect(lignes()).toHaveLength(3))
    expect(lire).toHaveBeenLastCalledWith(2)
    expect(page.queryByRole('button', { name: 'Plus anciens' })).toBeNull()
  })

  it('trophées : liste avec état, activer ou désactiver envoie le PATCH', async () => {
    const modifier = vi.spyOn(adminApi, 'modifier').mockImplementation(async (id, t) => ({ ...tropheesAdmin().find((x) => x.id === id)!, ...t }) as never)
    const vue = render(SablierAdmin, { onglet: 'trophees' })
    const page = within(await vue.findByRole('region', { name: 'Configuration des trophées' }))
    expect(await page.findByText('3 trophées, 2 actifs. Un trophée désactivé n’est plus affiché et revient s’il est réactivé.')).toBeInTheDocument()
    const ligne = within(page.getByRole('rowheader', { name: /Appel fleuve/ }).closest('tr')!)
    expect(ligne.getByText('caché')).toBeInTheDocument()
    expect(ligne.getByText('3 h')).toBeInTheDocument()
    const actif = ligne.getByRole('switch', { name: 'Appel fleuve actif' }) as HTMLInputElement
    expect(actif.checked).toBe(false)
    await fireEvent.click(actif)
    await waitFor(() => expect(modifier).toHaveBeenCalledWith(26, { active: true }))
    expect(page.getByRole('link', { name: 'Modifier Platine' })).toHaveAttribute('href', '#/stats/admin/trophees/6')
  })

  it('C7 : créer un trophée envoie le POST attendu, durées saisies en heures', async () => {
    const creer = vi.spyOn(adminApi, 'creer').mockImplementation(async (t) => ({ ...tropheesAdmin()[0], ...t, id: 99 }) as never)
    const vue = render(SablierAdmin, { onglet: 'trophees', detail: 'nouveau' })
    const page = within(await vue.findByRole('region', { name: 'Créer un trophée' }))
    await fireEvent.click(await page.findByRole('button', { name: 'Enregistrer' }))
    expect(page.getAllByText('De 1 à 60 caractères.').length).toBeGreaterThan(0)
    expect(creer).not.toHaveBeenCalled()
    await fireEvent.input(page.getByLabelText('Nom'), { target: { value: 'Mélomane' } })
    expect((page.getByLabelText('Clé') as HTMLInputElement).value).toBe('melomane')
    await fireEvent.input(page.getByLabelText('Description'), { target: { value: 'Du temps passé à écouter.' } })
    await fireEvent.click(page.getByRole('button', { name: 'Icône casque' }))
    await fireEvent.change(page.getByLabelText('Stat du catalogue'), { target: { value: 'sillon.seconds' } })
    await fireEvent.change(page.getByLabelText('Période'), { target: { value: 'month' } })
    expect(page.getByText('Seuils par palier, en heures')).toBeInTheDocument()
    await fireEvent.input(page.getByLabelText('Palier 1'), { target: { value: '10' } })
    await fireEvent.click(page.getByRole('button', { name: 'Ajouter un palier' }))
    await fireEvent.input(page.getByLabelText('Palier 2'), { target: { value: '5' } })
    await fireEvent.click(page.getByRole('button', { name: 'Enregistrer' }))
    expect(await page.findByText('Les paliers doivent être strictement croissants.')).toBeInTheDocument()
    await fireEvent.input(page.getByLabelText('Palier 2'), { target: { value: '50,5' } })
    await fireEvent.click(page.getByRole('switch', { name: 'Trophée caché' }))
    await fireEvent.click(page.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(creer).toHaveBeenCalledTimes(1))
    expect(creer).toHaveBeenCalledWith({ key: 'melomane', name: 'Mélomane', description: 'Du temps passé à écouter.', icon: 'casque', hidden: true, active: true, kind: 'tiers', stat: 'sillon.seconds', period: 'month', thresholds: [36_000, 181_800] })
    await waitFor(() => expect(router.current).toMatchObject({ name: 'stats', section: 'admin', id: 'trophees', detail: undefined }))
  })

  it('modifier un trophée n’envoie que les champs changés, et affiche le refus du serveur', async () => {
    const modifier = vi.spyOn(adminApi, 'modifier').mockRejectedValueOnce(new ApiError(400, 'invalid', 'thresholds : strictement croissants')).mockImplementation(async (_id, t) => ({ ...tropheesAdmin()[0], ...t }) as never)
    const vue = render(SablierAdmin, { onglet: 'trophees', detail: '6' })
    const page = within(await vue.findByRole('region', { name: 'Modifier un trophée' }))
    expect(await page.findByRole('heading', { name: 'Modifier « Platine »' })).toBeInTheDocument()
    expect(page.queryByLabelText('Clé')).toBeNull()
    expect(page.queryByRole('button', { name: 'Exploit codé' })).toBeNull()
    expect((page.getByLabelText('Palier 2') as HTMLInputElement).value).toBe('100')
    await fireEvent.input(page.getByLabelText('Palier 2'), { target: { value: '90' } })
    await fireEvent.click(page.getByRole('button', { name: 'Enregistrer' }))
    expect(await page.findByRole('alert')).toHaveTextContent('Le serveur a refusé : thresholds : strictement croissants')
    await fireEvent.click(page.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(modifier).toHaveBeenCalledTimes(2))
    expect(modifier).toHaveBeenLastCalledWith(6, { thresholds: [86_400, 324_000, 1_800_000, 7_200_000] })
  })

  it('exploit codé : le paramètre suit l’exploit choisi et part dans son unité', async () => {
    const creer = vi.spyOn(adminApi, 'creer').mockImplementation(async (t) => ({ ...tropheesAdmin()[1], ...t, id: 98 }) as never)
    const vue = render(SablierAdmin, { onglet: 'trophees', detail: 'nouveau' })
    const page = within(await vue.findByRole('region', { name: 'Créer un trophée' }))
    await fireEvent.input(await page.findByLabelText('Nom'), { target: { value: 'Bavard' } })
    await fireEvent.input(page.getByLabelText('Description'), { target: { value: 'Un très long appel.' } })
    await fireEvent.click(page.getByRole('button', { name: 'Exploit codé' }))
    expect((page.getByLabelText('Paramètre, en heures') as HTMLInputElement).value).toBe('3')
    await fireEvent.input(page.getByLabelText('Paramètre, en heures'), { target: { value: '5' } })
    await fireEvent.click(page.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(creer).toHaveBeenCalledWith({ key: 'bavard', name: 'Bavard', description: 'Un très long appel.', icon: 'trophee', hidden: false, active: true, kind: 'exploit', exploit: 'long_appel', parameter: 18_000 }))
    await fireEvent.change(page.getByLabelText('Exploit codé'), { target: { value: 'mois_parfait' } })
    expect(page.queryByLabelText(/^Paramètre/)).toBeNull()
  })
})
