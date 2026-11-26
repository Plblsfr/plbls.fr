import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { operation, OPERATIONS, page, releve } from './essais'
import TirelireOperation from './TirelireOperation.svelte'
import TirelireOperations from './TirelireOperations.svelte'
import TirelireSpace from './TirelireSpace.svelte'
import { tirelireApi, type Filtres, type Releve } from './tirelire'
import { tirelire } from './tirelire.svelte'

const texte = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''
const saisir = async (champ: HTMLElement, value: string) => { await fireEvent.input(champ, { target: { value } }); await fireEvent.change(champ) }

beforeEach(() => tirelire.reinitialiser())
afterEach(() => { vi.restoreAllMocks(); tirelire.reinitialiser() })

async function preparer(releves: Releve[] = [releve()]) {
  vi.spyOn(tirelireApi, 'releves').mockResolvedValue(releves)
  await tirelire.ouvrir()
}

describe('TirelireOperations', () => {
  it('groupe les opérations par jour, avec le total du jour et le sens de chaque montant', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    const liste = within(await vue.findByRole('region', { name: 'Liste des opérations' }))
    expect(liste.getAllByRole('heading', { level: 3 }).map(texte)).toEqual(['Jeudi 26 novembre−35,10 €', 'Mardi 24 novembre−291,80 €', 'Lundi 23 novembre+2 640,00 €'])
    const jeudi = within(liste.getByRole('list', { name: 'Jeudi 26 novembre' })).getAllByRole('link')
    expect(jeudi.map(texte)).toEqual(['MARCHE COLLINE VILLENEUVECarte −31,20 €', 'CAFE TAMIS VILLENEUVECarte −3,90 €'])
    expect(jeudi[0]).toHaveAttribute('href', '#/tirelire/operations/1')
    expect(texte(within(liste.getByRole('list', { name: 'Lundi 23 novembre' })).getByRole('link'))).toBe('VIR ATELIER MERIDIEN SALAIREVirement +2 640,00 €')
    expect(texte(vue.getByRole('group', { name: 'Résumé' }))).toBe('Opérations5 Débits−326,90 € Crédits+2 640,00 €')
    expect(vue.getByText('Toutes les opérations sont affichées')).toBeInTheDocument()
  })

  it('charge la suite à la demande ; le total d’un jour incomplet et les totaux inexacts ne sont pas affichés', async () => {
    await preparer()
    const tout = Array.from({ length: 60 }, (_, i) => operation({ id: i + 1, date: i < 50 ? '2026-11-20' : '2026-11-19', amount: 100 }))
    const lire = vi.spyOn(tirelireApi, 'operations').mockImplementation(async (_f: Filtres, offset = 0) => page(tout.slice(offset, offset + 50), 60, offset))
    const vue = render(TirelireOperations, { annee: 2026 })
    const suite = await vue.findByRole('button', { name: 'Afficher la suite (10)' })
    expect(vue.getAllByRole('heading', { level: 3 }).map(texte)).toEqual(['Vendredi 20 novembre'])
    expect(texte(vue.getByRole('group', { name: 'Résumé' }))).toBe('Opérations60')
    await fireEvent.click(suite)
    await waitFor(() => expect(vue.getAllByRole('heading', { level: 3 }).map(texte)).toEqual(['Vendredi 20 novembre−50,00 €', 'Jeudi 19 novembre−10,00 €']))
    expect(vue.queryByRole('button', { name: /Afficher la suite/ })).toBeNull()
    expect(texte(vue.getByRole('group', { name: 'Résumé' }))).toBe('Opérations60 Débits−60,00 € Crédits+0,00 €')
    expect(lire.mock.calls.map((c) => c[1])).toEqual([0, 50])
  })

  it('filtre par relevé et par période, puis réinitialise', async () => {
    await preparer([releve(), releve({ id: 10, number: 10, to: '2026-10-31' })])
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    const releves = vue.getByRole('combobox', { name: 'Relevé' })
    expect(within(releves).getAllByRole('option').map(texte)).toEqual(['Tous les relevés', 'Relevé n° 11 de 2026 · 26 nov. 2026', 'Relevé n° 10 de 2026 · 31 oct. 2026'])
    await fireEvent.change(releves, { target: { value: '10' } })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith({ releve: 10, du: '', au: '' }, 0))
    await fireEvent.click(vue.getByRole('button', { name: 'Afficher les filtres de période' }))
    await saisir(vue.getByLabelText('Du'), '2026-10-05')
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith({ releve: 10, du: '2026-10-05', au: '' }, 0))
    await saisir(vue.getByLabelText('Au'), '2026-10-01')
    expect(await vue.findByText('La date de début est après la date de fin.')).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(3)
    await saisir(vue.getByLabelText('Au'), '2026-10-20')
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith({ releve: 10, du: '2026-10-05', au: '2026-10-20' }, 0))
    await fireEvent.click(vue.getByRole('button', { name: 'Réinitialiser' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith({ releve: null, du: '', au: '' }, 0))
    expect((vue.getByLabelText('Du') as HTMLInputElement).value).toBe('')
  })

  it('ne propose ni recherche, ni filtre par catégorie, commerçant ou montant, ni export', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    await fireEvent.click(vue.getByRole('button', { name: 'Afficher les filtres de période' }))
    expect(vue.queryByRole('searchbox')).toBeNull()
    expect(vue.queryByText(/Catégorie|Commerçant|Montant|CSV|à classer/)).toBeNull()
  })

  it('sans relevé, invite à importer ; sans résultat pour un filtre, le dit', async () => {
    await preparer([])
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page([]))
    const vide = render(TirelireOperations)
    expect(await vide.findByRole('link', { name: 'Importer un relevé PDF' })).toHaveAttribute('href', '#/tirelire/import')
    vide.unmount()
    tirelire.reinitialiser()
    await preparer()
    lire.mockResolvedValue(page([]))
    await tirelire.filtrer({ du: '2020-01-01', au: '2020-01-31' })
    const vue = render(TirelireOperations)
    expect(await vue.findByText('Rien ne correspond à ces filtres.')).toBeInTheDocument()
    expect((vue.getByLabelText('Du') as HTMLInputElement).value).toBe('2020-01-01')
  })

  it('403, panne ou réseau : message lisible et relance', async () => {
    await preparer()
    const lire = vi.spyOn(tirelireApi, 'operations').mockRejectedValueOnce(new ApiError(403, 'forbidden', 'permission manquante')).mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    expect(texte(await vue.findByRole('alert'))).toContain('Tu n’as pas accès à Tirelire.')
    await fireEvent.click(vue.getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('region', { name: 'Liste des opérations' })).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(2)
  })
})

describe('TirelireOperation', () => {
  it('détaille une opération par carte : libellé brut, détails, dates, section, type et relevé', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperation, { id: 3 })
    const resume = await vue.findByRole('region', { name: 'Résumé de l’opération' })
    expect(texte(resume)).toBe('LIVREO*CMD PARISCarte −41,80 €')
    const releveVu = within(vue.getByRole('region', { name: 'Libellé du relevé' }))
    expect(releveVu.getByText('CARTE 22/11 LIVREO*CMD PARIS')).toBeInTheDocument()
    expect(releveVu.getByText('Détails (2 lignes)')).toBeInTheDocument()
    expect(releveVu.getByText(/LIVREO\*CMD 88213/).textContent).toBe('LIVREO*CMD 88213\nREF 0KX72A91')
    expect([...vue.container.querySelectorAll('.tir-kv')].map((kv) => [...kv.children].map(texte))).toEqual([
      ['Date d’achat carte', 'dimanche 22 nov. 2026'],
      ['Date de l’opération', 'mardi 24 nov. 2026'],
      ['Date de valeur', 'mardi 24 nov. 2026'],
      ['Section du relevé', 'PAIEMENTS PAR CARTE'],
      ['Type', 'Carte'],
      ['Sens', 'Débit'],
      ['Source', 'Relevé n° 11 de 2026 · du 31 oct. au 26 nov. 2026'],
    ])
  })

  it('un virement reçu n’a pas de date d’achat ; un seul détail s’accorde', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperation, { id: 5 })
    expect(texte(await vue.findByRole('region', { name: 'Résumé de l’opération' }))).toBe('VIR ATELIER MERIDIEN SALAIREVirement +2 640,00 €')
    expect(vue.getByText('Détails (1 ligne)')).toBeInTheDocument()
    expect(vue.queryByText('Date d’achat carte')).toBeNull()
    expect(vue.getByText('Crédit')).toBeInTheDocument()
  })

  it('opération absente de la liste chargée : le dit et renvoie à la liste', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperation, { id: 999 })
    expect(await vue.findByRole('heading', { name: 'Opération introuvable' })).toBeInTheDocument()
    expect(vue.getByRole('link', { name: 'Retour aux opérations' })).toHaveAttribute('href', '#/tirelire/operations')
  })
})

describe('TirelireSpace', () => {
  const monter = (section?: string, id?: string) => render(TirelireSpace, { section, id, onmenu: () => {} })

  it('premier lancement : aucun relevé, invitation à importer et navigation à cinq onglets', async () => {
    vi.spyOn(tirelireApi, 'releves').mockResolvedValue([])
    const vue = monter()
    expect(await vue.findByRole('heading', { name: 'Aucun relevé pour l’instant' })).toBeInTheDocument()
    expect(vue.getByRole('heading', { name: 'Bienvenue', level: 1 })).toBeInTheDocument()
    expect(vue.getByRole('link', { name: 'Importer un relevé PDF' })).toHaveAttribute('href', '#/tirelire/import')
    const onglets = within(vue.getByRole('navigation', { name: 'Sections de Tirelire' })).getAllByRole('link')
    expect(onglets.map((a) => [texte(a), a.getAttribute('href'), a.getAttribute('aria-current')])).toEqual([
      ['Accueil', '#/tirelire', 'page'], ['Mois', '#/tirelire/mois', null], ['Évolution', '#/tirelire/evolution', null], ['Habitudes', '#/tirelire/habitudes', null], ['Opérations', '#/tirelire/operations', null],
    ])
  })

  it('Accueil, Mois, Évolution et Habitudes disent attendre les catégories, sans chiffre inventé', async () => {
    vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve(), releve({ id: 10, number: 10, from: '2026-09-30', to: '2026-10-31', operations: 91 })])
    for (const [section, titre] of [[undefined, 'Accueil'], ['mois', 'Vue du mois'], ['evolution', 'Évolution'], ['habitudes', 'Habitudes']] as const) {
      const vue = monter(section)
      expect(await vue.findByRole('heading', { name: 'Disponible quand les catégories seront en place' })).toBeInTheDocument()
      expect(vue.getByRole('heading', { name: titre, level: 1 })).toBeInTheDocument()
      expect(vue.container.textContent).not.toMatch(/€/)
      expect(texte(vue.getByRole('link', { name: /^Opérations\s*175/ }))).toContain('175 opérations · du 30 sept. au 26 nov. 2026')
      vue.unmount()
    }
  })

  it('Tirelire injoignable : message et relance, sans bloquer la navigation', async () => {
    const lire = vi.spyOn(tirelireApi, 'releves').mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValue([])
    const vue = monter()
    expect(texte(await vue.findByRole('alert'))).toContain('Tirelire est injoignable. Vérifie ta connexion. Le reste de l’app fonctionne normalement.')
    expect(vue.getByRole('navigation', { name: 'Sections de Tirelire' })).toBeInTheDocument()
    await fireEvent.click(vue.getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('heading', { name: 'Aucun relevé pour l’instant' })).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(2)
  })

  it('l’en-tête du détail donne la date et la nature de l’opération', async () => {
    vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve()])
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page(OPERATIONS))
    const vue = monter('operations', '3')
    expect(await vue.findByText('24 nov. 2026 · débit · carte')).toBeInTheDocument()
    expect(vue.getByRole('link', { name: 'Retour aux opérations' })).toHaveAttribute('href', '#/tirelire/operations')
    expect(within(vue.getByRole('navigation', { name: 'Sections de Tirelire' })).getByRole('link', { name: 'Opérations' })).toHaveAttribute('aria-current', 'page')
  })
})
