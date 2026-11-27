import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { CATEGORIES, commercant, moisStats, operation, OPERATIONS, page, releve } from './essais'
import TirelireOperation from './TirelireOperation.svelte'
import TirelireOperations from './TirelireOperations.svelte'
import TirelireSpace from './TirelireSpace.svelte'
import { DELAI_RECHERCHE, filtres, tirelireApi, type Filtres, type Releve } from './tirelire'
import { tirelire } from './tirelire.svelte'

const texte = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''
const saisir = async (champ: HTMLElement, value: string) => { await fireEvent.input(champ, { target: { value } }); await fireEvent.change(champ) }

beforeEach(() => {
  tirelire.reinitialiser()
  vi.spyOn(tirelireApi, 'categories').mockResolvedValue(CATEGORIES)
  vi.spyOn(tirelireApi, 'compteur').mockResolvedValue({ operations: 0, merchants: 0, debits: 0, credits: 0 })
  vi.spyOn(tirelireApi, 'commercants').mockResolvedValue([commercant({ id: 7, name: 'Cafe Tamis Villeneuve' })])
  vi.spyOn(tirelireApi, 'mois').mockResolvedValue(moisStats())
  vi.spyOn(tirelireApi, 'periodes').mockResolvedValue([])
  vi.spyOn(tirelireApi, 'operation').mockImplementation(async (id: number) => {
    const o = OPERATIONS.find((x) => x.id === id)
    if (!o) throw new ApiError(404, 'not_found', 'introuvable')
    return o
  })
})
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
    expect(jeudi.map(texte)).toEqual(['MARCHE COLLINE VILLENEUVEà classerCarte −31,20 €', 'CAFE TAMIS VILLENEUVEà classerCarte −3,90 €'])
    expect(jeudi[0]).toHaveAttribute('href', '#/tirelire/operations/1')
    expect(texte(within(liste.getByRole('list', { name: 'Lundi 23 novembre' })).getByRole('link'))).toBe('VIR ATELIER MERIDIEN SALAIREà classerVirement +2 640,00 €')
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
    expect(texte(vue.getByRole('group', { name: 'Résumé' }))).toBe('Opérations60 Débits−60,00 € Crédits0,00 €')
    expect(lire.mock.calls.map((c) => c[1])).toEqual([0, 50])
  })

  it('filtre par relevé et par période, puis réinitialise', async () => {
    await preparer([releve(), releve({ id: 10, number: 10, to: '2026-10-31' })])
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    await fireEvent.click(vue.getByRole('button', { name: 'Afficher les filtres' }))
    const releves = vue.getByLabelText('Relevé')
    expect(within(releves).getAllByRole('option').map(texte)).toEqual(['Tous les relevés', 'Relevé n° 11 de 2026 · 26 nov. 2026', 'Relevé n° 10 de 2026 · 31 oct. 2026'])
    await fireEvent.change(releves, { target: { value: '10' } })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ releve: 10, du: '', au: '' }), 0))
    await saisir(vue.getByLabelText('Du'), '2026-10-05')
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ releve: 10, du: '2026-10-05', au: '' }), 0))
    await saisir(vue.getByLabelText('Au'), '2026-10-01')
    expect(await vue.findByText('La date de début est après la date de fin.')).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(3)
    await saisir(vue.getByLabelText('Au'), '2026-10-20')
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ releve: 10, du: '2026-10-05', au: '2026-10-20' }), 0))
    await fireEvent.click(vue.getByRole('button', { name: 'Réinitialiser' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ releve: null, du: '', au: '' }), 0))
    expect((vue.getByLabelText('Du') as HTMLInputElement).value).toBe('')
  })

  it('montre la catégorie de chaque ligne, ou « à classer », et le nom du commerçant', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page([
      operation({ id: 1, merchant_id: 3, merchant_name: 'Marche Colline Villeneuve', category_id: 2, category_source: 'regle', rule_id: 4 }),
      operation({ id: 2, label: 'CARTE 26/11 KSQ 4471', merchant: 'KSQ 4471', amount: 240 }),
    ]))
    const vue = render(TirelireOperations, { annee: 2026 })
    const lignes = within(await vue.findByRole('region', { name: 'Liste des opérations' })).getAllByRole('link')
    expect(lignes.map(texte)).toEqual(['Marche Colline VilleneuveCoursesCarte −31,20 €', 'KSQ 4471à classerCarte −2,40 €'])
    expect(lignes[0].querySelector('.tir-pt--2')).not.toBeNull()
  })

  it('filtre par catégorie, par commerçant et « à classer »', async () => {
    await preparer()
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    await fireEvent.click(vue.getByRole('button', { name: 'Afficher les filtres' }))
    const categories = vue.getByLabelText('Catégorie')
    expect(within(categories).getAllByRole('option').map(texte)).toEqual(['Toutes', 'À classer', 'Logement', 'Courses', 'Restaurants', 'Épargne', 'Revenus'])
    await fireEvent.change(categories, { target: { value: '2' } })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ categorie: 2 }), 0))
    await fireEvent.change(categories, { target: { value: 'a-classer' } })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ aClasser: true }), 0))
    await fireEvent.change(await vue.findByLabelText('Commerçant'), { target: { value: '7' } })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ aClasser: true, commercant: 7 }), 0))
    await fireEvent.click(vue.getByRole('button', { name: 'Réinitialiser' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres(), 0))
  })

  it('recherche après un délai de saisie, tout de suite avec Entrée, et s’efface d’un bouton', async () => {
    await preparer()
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    expect(lire).toHaveBeenCalledTimes(1)
    vi.useFakeTimers()
    try {
      const champ = vue.getByRole('searchbox', { name: 'Rechercher une opération' })
      await fireEvent.input(champ, { target: { value: 'ca' } })
      await vi.advanceTimersByTimeAsync(DELAI_RECHERCHE - 50)
      await fireEvent.input(champ, { target: { value: 'café ' } })
      await vi.advanceTimersByTimeAsync(DELAI_RECHERCHE - 50)
      expect(lire).toHaveBeenCalledTimes(1)
      await vi.advanceTimersByTimeAsync(60)
      expect(lire).toHaveBeenCalledTimes(2)
      expect(lire).toHaveBeenLastCalledWith(filtres({ q: 'café' }), 0)
      expect((champ as HTMLInputElement).value).toBe('café ')
      expect(location.hash).toBe('#/tirelire/operations?q=caf%C3%A9')

      await fireEvent.input(champ, { target: { value: 'café tamis' } })
      await fireEvent.keyDown(champ, { key: 'Enter' })
      expect(lire).toHaveBeenCalledTimes(3)
      expect(lire).toHaveBeenLastCalledWith(filtres({ q: 'café tamis' }), 0)
      await vi.advanceTimersByTimeAsync(DELAI_RECHERCHE * 2)
      expect(lire).toHaveBeenCalledTimes(3)

      await fireEvent.click(vue.getByRole('button', { name: 'Effacer la recherche' }))
      expect(lire).toHaveBeenLastCalledWith(filtres(), 0)
      expect((champ as HTMLInputElement).value).toBe('')
      expect(vue.queryByRole('button', { name: 'Effacer la recherche' })).toBeNull()
      expect(location.hash).toBe('#/tirelire/operations')
    } finally {
      vi.useRealTimers()
    }
  })

  it('filtre par montant, par sens et par type ; un montant illisible ou inversé est dit sans rien demander', async () => {
    await preparer()
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    await fireEvent.click(vue.getByRole('button', { name: 'Afficher les filtres' }))
    await saisir(vue.getByRole('textbox', { name: 'Montant minimum' }), '10')
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ min: 1000 }), 0))
    await saisir(vue.getByRole('textbox', { name: 'Montant maximum' }), '5')
    expect(await vue.findByText('Le montant minimum dépasse le montant maximum.')).toBeInTheDocument()
    await saisir(vue.getByRole('textbox', { name: 'Montant maximum' }), 'douze')
    expect(await vue.findByText('Écris un montant en euros, par exemple 12,50.')).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(2)
    await saisir(vue.getByRole('textbox', { name: 'Montant maximum' }), '49,9')
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ min: 1000, max: 4990 }), 0))
    const sens = within(vue.getByRole('group', { name: 'Sens de l’opération' }))
    expect(sens.getAllByRole('button').map((b) => [texte(b), b.getAttribute('aria-pressed')])).toEqual([['Tout', 'true'], ['Débits', 'false'], ['Crédits', 'false']])
    await fireEvent.click(sens.getByRole('button', { name: 'Débits' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ min: 1000, max: 4990, sens: 'debit' }), 0))
    expect(sens.getByRole('button', { name: 'Débits' })).toHaveAttribute('aria-pressed', 'true')
    const types = vue.getByLabelText('Type')
    expect(within(types).getAllByRole('option').map(texte)).toEqual(['Tous', 'Carte', 'Prélèvement', 'Virement', 'Virement instantané', 'Échéance de prêt', 'Cotisation', 'Autre opération'])
    await fireEvent.change(types, { target: { value: 'prelevement' } })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ min: 1000, max: 4990, sens: 'debit', type: 'prelevement' }), 0))
    expect(location.hash).toBe('#/tirelire/operations?min=1000&max=4990&direction=debit&type=prelevement')
    await fireEvent.click(sens.getByRole('button', { name: 'Tout' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ min: 1000, max: 4990, type: 'prelevement' }), 0))
  })

  it('pastilles des filtres actifs : chacune retire son filtre, « Tout effacer » les retire tous', async () => {
    await preparer()
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    await tirelire.filtrer({ q: 'café', categorie: 2, min: 500, sens: 'credit', du: '2026-11-01' })
    const vue = render(TirelireOperations, { annee: 2026 })
    const actifs = within(await vue.findByRole('list', { name: 'Filtres actifs' }))
    expect(actifs.getAllByRole('button').map(texte)).toEqual(['« café »', 'Depuis le 1er nov. 2026', 'Courses', 'Au moins 5,00 €', 'Crédits', 'Tout effacer'])
    expect((vue.getByRole('searchbox') as HTMLInputElement).value).toBe('café')
    expect((vue.getByRole('textbox', { name: 'Montant minimum' }) as HTMLInputElement).value).toBe('5')
    expect(location.hash).toBe('#/tirelire/operations?from=2026-11-01&category=2&q=caf%C3%A9&min=500&direction=credit')
    await fireEvent.click(actifs.getByRole('button', { name: /^Retirer le filtre Au moins 5,00\s€$/ }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ q: 'café', categorie: 2, sens: 'credit', du: '2026-11-01' }), 0))
    expect((vue.getByRole('textbox', { name: 'Montant minimum' }) as HTMLInputElement).value).toBe('')
    await fireEvent.click(actifs.getByRole('button', { name: 'Retirer le filtre « café »' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ categorie: 2, sens: 'credit', du: '2026-11-01' }), 0))
    expect((vue.getByRole('searchbox') as HTMLInputElement).value).toBe('')
    expect(actifs.getAllByRole('button').map(texte)).toEqual(['Depuis le 1er nov. 2026', 'Courses', 'Crédits', 'Tout effacer'])
    await fireEvent.click(actifs.getByRole('button', { name: 'Tout effacer' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres(), 0))
    expect(vue.queryByRole('list', { name: 'Filtres actifs' })).toBeNull()
    expect(location.hash).toBe('#/tirelire/operations')
  })

  it('les filtres viennent de l’adresse et la suivent', async () => {
    await preparer()
    const lire = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperations, { annee: 2026, requete: 'q=caf%C3%A9&min=500&direction=debit&type=cheque' })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    expect(lire).toHaveBeenCalledTimes(1)
    expect(lire).toHaveBeenLastCalledWith(filtres({ q: 'café', min: 500, sens: 'debit' }), 0)
    expect((vue.getByRole('searchbox') as HTMLInputElement).value).toBe('café')
    expect(vue.getByRole('button', { name: 'Masquer les filtres' })).toBeInTheDocument()
    expect(within(vue.getByRole('group', { name: 'Sens de l’opération' })).getByRole('button', { name: 'Débits' })).toHaveAttribute('aria-pressed', 'true')
    await vue.rerender({ requete: 'q=tamis' })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres({ q: 'tamis' }), 0))
    expect((vue.getByRole('searchbox') as HTMLInputElement).value).toBe('tamis')
    await vue.rerender({ requete: 'q=tamis' })
    expect(lire).toHaveBeenCalledTimes(2)
    await vue.rerender({ requete: '' })
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith(filtres(), 0))
    expect(lire).toHaveBeenCalledTimes(3)
  })

  it('affiche le total de la sélection rendu par le service, même sans avoir tout chargé', async () => {
    await preparer()
    const tout = Array.from({ length: 50 }, (_, i) => operation({ id: i + 1, amount: 100 }))
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue({ ...page(tout, 1284), total_debits: 1_294_800, total_credits: 264_000 })
    const vue = render(TirelireOperations, { annee: 2026 })
    expect(texte(await vue.findByRole('group', { name: 'Résumé' }))).toBe('Opérations1 284 Débits−12 948,00 € Crédits+2 640,00 €')
    vue.unmount()
    await tirelire.filtrer({ sens: 'debit' })
    const filtree = render(TirelireOperations, { annee: 2026 })
    expect(texte(await filtree.findByRole('group', { name: 'Total de la sélection' }))).toBe('Opérations1 284 Débits−12 948,00 € Crédits+2 640,00 €')
  })

  it('exporte en CSV la sélection, dit le fichier enregistré ou l’échec', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: vi.fn(() => 'blob:essai'), revokeObjectURL: vi.fn() }))
    const clic = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    const exporter = vi.spyOn(tirelireApi, 'exporter').mockResolvedValueOnce({ blob: new Blob(['a;b']), nom: 'tirelire-operations-2026-11-23_2026-11-26.csv' }).mockRejectedValueOnce(new ApiError(503, 'unavailable', 'indisponible'))
    await tirelire.filtrer({ sens: 'debit' })
    const vue = render(TirelireOperations, { annee: 2026 })
    await vue.findByRole('region', { name: 'Liste des opérations' })
    await fireEvent.click(vue.getByRole('button', { name: 'Exporter en CSV' }))
    expect(texte(await vue.findByRole('status'))).toBe('5 opérations exportées dans tirelire-operations-2026-11-23_2026-11-26.csv.')
    expect(exporter).toHaveBeenLastCalledWith(filtres({ sens: 'debit' }))
    expect(clic).toHaveBeenCalledTimes(1)
    await fireEvent.click(vue.getByRole('button', { name: 'Exporter en CSV' }))
    expect(texte(await vue.findByRole('alert'))).toBe('L’export a échoué. Tirelire est indisponible pour le moment.')
    expect(vue.queryByRole('status')).toBeNull()
    vi.unstubAllGlobals()
  })

  it('sans opération dans la sélection, l’export est désactivé et la recherche vide est nommée', async () => {
    await preparer()
    vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page([]))
    await tirelire.filtrer({ q: 'introuvable', sens: 'debit' })
    const vue = render(TirelireOperations, { annee: 2026 })
    expect(await vue.findByText('Rien ne correspond à « introuvable » avec ces filtres.')).toBeInTheDocument()
    expect(vue.getByRole('button', { name: 'Exporter en CSV' })).toBeDisabled()
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
    expect(texte(resume)).toBe('LIVREO*CMD PARISà classerCarte −41,80 €')
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
    expect(texte(await vue.findByRole('region', { name: 'Résumé de l’opération' }))).toBe('VIR ATELIER MERIDIEN SALAIREà classerVirement +2 640,00 €')
    expect(vue.getByText('Détails (1 ligne)')).toBeInTheDocument()
    expect(vue.queryByText('Date d’achat carte')).toBeNull()
    expect(texte(vue.getByText('Sens').parentElement)).toBe('SensCrédit')
  })

  it('lien direct : l’opération est lue seule, sans charger la liste', async () => {
    await preparer()
    const liste = vi.spyOn(tirelireApi, 'operations').mockResolvedValue(page())
    const vue = render(TirelireOperation, { id: 4 })
    expect(texte(await vue.findByRole('region', { name: 'Résumé de l’opération' }))).toContain('VIR VERS LIVRET')
    expect(tirelireApi.operation).toHaveBeenCalledWith(4)
    expect(liste).not.toHaveBeenCalled()
  })

  it('reclasse tout le commerçant, ou cette opération seulement', async () => {
    await preparer()
    const o = operation({ id: 8, merchant_id: 3, merchant_name: 'Marche Colline Villeneuve', category_id: 2, category_source: 'regle', rule_id: 4 })
    vi.mocked(tirelireApi.operation).mockResolvedValue(o)
    const classer = vi.spyOn(tirelireApi, 'classerOperation').mockImplementation(async (_id, categorie, portee) => ({ operation: { ...o, category_id: categorie, category_source: portee === 'merchant' ? 'commercant' : 'operation', rule_id: null }, reclassified: portee === 'merchant' ? 31 : 1 }))
    const vue = render(TirelireOperation, { id: 8 })
    const carte = within(await vue.findByRole('region', { name: 'Catégorie' }))
    expect(carte.getByText('règle par mots-clés')).toBeInTheDocument()
    const choix = carte.getByRole('combobox', { name: 'Catégorie de l’opération' }) as HTMLSelectElement
    expect(choix.value).toBe('2')
    expect(carte.getByRole('button', { name: 'Tout le commerçant' })).toHaveAttribute('aria-pressed', 'true')
    expect(carte.getByRole('button', { name: 'Appliquer à tout le commerçant' })).toBeDisabled()
    await fireEvent.change(choix, { target: { value: '3' } })
    await fireEvent.click(carte.getByRole('button', { name: 'Appliquer à tout le commerçant' }))
    await waitFor(() => expect(classer).toHaveBeenLastCalledWith(8, 3, 'merchant'))
    expect(texte(await carte.findByRole('status'))).toBe('Règle enregistrée pour tout le commerçant : 31 opérations reclassées.')
    expect(carte.getByText('règle du commerçant')).toBeInTheDocument()
    await fireEvent.click(carte.getByRole('button', { name: 'Cette opération' }))
    await fireEvent.change(choix, { target: { value: '1' } })
    await fireEvent.click(carte.getByRole('button', { name: 'Appliquer à cette opération' }))
    await waitFor(() => expect(classer).toHaveBeenLastCalledWith(8, 1, 'operation'))
    await waitFor(() => expect(texte(carte.getByRole('status'))).toBe('Catégorie enregistrée pour cette opération seulement.'))
    expect(carte.getByText('choisie pour cette opération')).toBeInTheDocument()
  })

  it('sans commerçant, seule cette opération se classe ; un refus s’affiche', async () => {
    await preparer()
    vi.mocked(tirelireApi.operation).mockResolvedValue(operation({ id: 9 }))
    vi.spyOn(tirelireApi, 'classerOperation').mockRejectedValue(new ApiError(500, 'internal', 'erreur interne'))
    const vue = render(TirelireOperation, { id: 9 })
    const carte = within(await vue.findByRole('region', { name: 'Catégorie' }))
    expect(carte.queryByRole('button', { name: 'Tout le commerçant' })).toBeNull()
    await fireEvent.change(carte.getByRole('combobox'), { target: { value: '2' } })
    await fireEvent.click(carte.getByRole('button', { name: 'Appliquer à cette opération' }))
    expect(texte(await carte.findByRole('alert'))).toBe('Tirelire est indisponible pour le moment.')
  })

  it('opération inconnue : le dit et renvoie à la liste', async () => {
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

  it('Habitudes reste en attente, sans chiffre inventé', async () => {
    vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve(), releve({ id: 10, number: 10, from: '2026-09-30', to: '2026-10-31', operations: 91 })])
    for (const [section, titre] of [['habitudes', 'Habitudes']] as const) {
      const vue = monter(section)
      expect(await vue.findByRole('heading', { name: 'Bientôt disponible' })).toBeInTheDocument()
      expect(vue.getByRole('heading', { name: titre, level: 1 })).toBeInTheDocument()
      expect(vue.container.textContent).not.toMatch(/€/)
      expect(vue.queryByRole('link', { name: /à classer/i })).toBeNull()
      expect(texte(vue.getByRole('link', { name: /^Opérations\s*175/ }))).toContain('175 opérations · du 30 sept. au 26 nov. 2026')
      expect(vue.getByRole('link', { name: /Catégories et règles/ })).toHaveAttribute('href', '#/tirelire/categories')
      expect(vue.getByRole('link', { name: /^Vue du mois/ })).toHaveAttribute('href', '#/tirelire/mois')
      expect(within(vue.getByRole('region', { name: 'Déjà disponible' })).getByRole('link', { name: /^Évolution/ })).toHaveAttribute('href', '#/tirelire/evolution')
      vue.unmount()
      tirelire.reinitialiser()
    }
  })

  it('bandeau « à classer » sur l’accueil et pastille dans l’en-tête tant que la file n’est pas vide', async () => {
    vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve()])
    vi.mocked(tirelireApi.compteur).mockResolvedValue({ operations: 7, merchants: 5, debits: 2800, credits: 0 })
    const vue = monter()
    const bandeau = await vue.findByRole('link', { name: /^7 opérations à classer/ })
    expect(bandeau).toHaveAttribute('href', '#/tirelire/a-classer')
    expect(texte(bandeau)).toBe('7 opérations à classer28,00 € de débits sans catégorie · 5 commerçants')
    vue.unmount()
    const mois = monter('mois')
    expect(await mois.findByRole('heading', { name: 'Par catégorie' })).toBeInTheDocument()
    expect(mois.getByRole('link', { name: 'À classer : 7 opérations' })).toHaveAttribute('href', '#/tirelire/a-classer')
    expect(mois.container.querySelector('.tir-bandeau')).toBeNull()
  })

  it('premier lancement : plus aucune étape « à venir »', async () => {
    vi.spyOn(tirelireApi, 'releves').mockResolvedValue([])
    const vue = monter()
    const etapes = within(await vue.findByRole('region', { name: 'Comment ça marche' }))
    expect(texte(etapes.getByText('Classe l’inconnu').closest('.tir-li'))).toBe('2Classe l’inconnuUn choix crée une règle pour tout le commerçant.')
    expect(etapes.queryByText('à venir')).toBeNull()
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
    const vue = monter('operations', '3')
    expect(await vue.findByText('24 nov. 2026 · débit · carte')).toBeInTheDocument()
    expect(vue.getByRole('link', { name: 'Retour aux opérations' })).toHaveAttribute('href', '#/tirelire/operations')
    expect(within(vue.getByRole('navigation', { name: 'Sections de Tirelire' })).getByRole('link', { name: 'Opérations' })).toHaveAttribute('aria-current', 'page')
  })
})
