import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { confirmation } from '../../lib/confirm.svelte'
import { categorie, CATEGORIES, commercant, file, groupe, operation, regle } from './essais'
import TirelireCategories from './TirelireCategories.svelte'
import TirelireClasser from './TirelireClasser.svelte'
import { tirelireApi, type Categorie, type Commercant, type Groupe } from './tirelire'
import { tirelire } from './tirelire.svelte'

const texte = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''
const confirmerLaBoite = async () => { await waitFor(() => expect(confirmation.en).not.toBeNull()); confirmation.en?.resoudre({ ok: true, valeur: '' }) }

let categories: Categorie[]

beforeEach(() => {
  tirelire.reinitialiser()
  categories = [...CATEGORIES]
  vi.spyOn(tirelireApi, 'categories').mockImplementation(async () => categories)
  vi.spyOn(tirelireApi, 'compteur').mockResolvedValue({ operations: 0, merchants: 0, debits: 0, credits: 0 })
})
afterEach(() => { confirmation.en?.resoudre({ ok: false, valeur: '' }); vi.restoreAllMocks(); tirelire.reinitialiser() })

describe('TirelireClasser', () => {
  const seul = groupe({ merchant_id: null, merchant_name: null, operations: 1, debits: 6000, sample: [operation({ id: 9, type: 'autre', label: 'RETRAIT DAB 12/11', merchant: null, amount: 6000 })] })

  function preparer(groupes: Groupe[]) {
    const restants = [...groupes]
    vi.spyOn(tirelireApi, 'aClasser').mockImplementation(async () => file([...restants]))
    return restants
  }

  it('présente le premier groupe, classe tout le commerçant, confirme et permet d’annuler', async () => {
    const restants = preparer([groupe(), seul])
    const classer = vi.spyOn(tirelireApi, 'classerCommercant').mockImplementation(async (_id, c) => { if (c !== null) restants.shift(); else restants.unshift(groupe()); return { merchant: commercant(), reclassified: 2 } })
    const vue = render(TirelireClasser)
    const carte = within(await vue.findByRole('region', { name: 'À classer maintenant' }))
    expect(carte.getByRole('heading', { level: 2 }).textContent).toBe('Cafe Tamis Villeneuve')
    expect(texte(vue.getByRole('group', { name: 'Avancement' }))).toBe('0 classée sur 30 %')
    expect(carte.getByText('CARTE 26/11 CAFE TAMIS VILLENEUVE')).toBeInTheDocument()
    expect(within(carte.getByRole('group', { name: 'Catégories' })).getAllByRole('button').map(texte)).toEqual(['Courses', 'Restaurants', 'Logement', 'Épargne', 'Revenus'])
    const valider = carte.getByRole('button', { name: 'Choisis une catégorie' })
    expect(valider).toBeDisabled()
    await fireEvent.click(carte.getByRole('button', { name: 'Restaurants' }))
    expect(carte.getByRole('button', { name: 'Restaurants' })).toHaveAttribute('aria-pressed', 'true')
    expect(texte(carte.getByRole('checkbox').closest('label'))).toBe('Créer une règle : toutes les opérations « Cafe Tamis Villeneuve » iront en Restaurants')
    await fireEvent.click(carte.getByRole('button', { name: 'Classer en Restaurants' }))
    await waitFor(() => expect(classer).toHaveBeenCalledWith(7, 3))
    const fait = await vue.findByRole('status')
    expect(texte(fait)).toBe('Règle créée pour tout le commerçant« Cafe Tamis Villeneuve » → Restaurants · 2 opérations reclassées Annuler')
    await waitFor(() => expect(texte(vue.getByRole('group', { name: 'Avancement' }))).toBe('2 classées sur 367 %'))
    expect(vue.getByRole('region', { name: 'À classer maintenant' })).toHaveTextContent('RETRAIT DAB 12/11')
    await fireEvent.click(within(fait).getByRole('button', { name: 'Annuler' }))
    await waitFor(() => expect(classer).toHaveBeenLastCalledWith(7, null))
    await waitFor(() => expect(vue.queryByRole('status')).toBeNull())
    await waitFor(() => expect(vue.getByRole('region', { name: 'À classer maintenant' })).toHaveTextContent('Cafe Tamis Villeneuve'))
  })

  it('une opération sans commerçant se classe seule ; « Passer » et « Ensuite » changent de groupe', async () => {
    preparer([seul, groupe()])
    const classer = vi.spyOn(tirelireApi, 'classerOperation').mockResolvedValue({ operation: operation(), reclassified: 1 })
    const vue = render(TirelireClasser)
    const carte = () => within(vue.getByRole('region', { name: 'À classer maintenant' }))
    await vue.findByRole('region', { name: 'À classer maintenant' })
    expect(carte().queryByRole('checkbox')).toBeNull()
    expect(carte().getByText(/l’opération se classe seule/)).toBeInTheDocument()
    const ensuite = within(vue.getByRole('region', { name: 'Ensuite' }))
    expect(ensuite.getAllByRole('button').map(texte)).toEqual(['Cafe Tamis Villeneuve2 opérations · du 20 au 26 nov. 2026 −7,80 €'])
    await fireEvent.click(carte().getByRole('button', { name: 'Passer' }))
    expect(carte().getByRole('heading', { level: 2 }).textContent).toBe('Cafe Tamis Villeneuve')
    await fireEvent.click(within(vue.getByRole('region', { name: 'Ensuite' })).getByRole('button'))
    expect(carte().getByRole('heading', { level: 2 }).textContent).toBe('RETRAIT DAB 12/11')
    await fireEvent.click(carte().getByRole('button', { name: 'Courses' }))
    await fireEvent.click(carte().getByRole('button', { name: 'Classer en Courses' }))
    await waitFor(() => expect(classer).toHaveBeenCalledWith(9, 2, 'operation'))
  })

  it('décocher la règle ne classe que l’opération la plus récente du commerçant', async () => {
    preparer([groupe()])
    const classer = vi.spyOn(tirelireApi, 'classerOperation').mockResolvedValue({ operation: operation(), reclassified: 1 })
    const vue = render(TirelireClasser)
    const carte = within(await vue.findByRole('region', { name: 'À classer maintenant' }))
    await fireEvent.click(carte.getByRole('checkbox'))
    await fireEvent.click(carte.getByRole('button', { name: 'Logement' }))
    await fireEvent.click(carte.getByRole('button', { name: 'Classer en Logement' }))
    await waitFor(() => expect(classer).toHaveBeenCalledWith(2, 1, 'operation'))
    expect(texte(await vue.findByRole('status'))).toContain('Opération classée')
  })

  it('file vide : tout est classé ; panne : message et relance', async () => {
    const lire = vi.spyOn(tirelireApi, 'aClasser').mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValue(file([]))
    const vue = render(TirelireClasser)
    expect(texte(await vue.findByRole('alert'))).toContain('Tirelire est injoignable.')
    await fireEvent.click(vue.getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('heading', { name: 'Tout est classé' })).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(2)
  })
})

describe('TirelireCategories', () => {
  it('liste les catégories avec leur nature et leur plafond, et le budget du mois', async () => {
    const vue = render(TirelireCategories, { volet: 'categories' })
    const liste = within(await vue.findByRole('region', { name: 'Catégories' }))
    expect(liste.getAllByRole('listitem').map(texte)).toEqual(['Logement fixe 780,00 €', 'Courses variable 380,00 €', 'Restaurants variable 150,00 €', 'Épargne épargne —', 'Revenus revenu —', 'Divers archivée —'])
    expect(texte(liste.getByText('Budget du mois').parentElement)).toBe('Budget du mois1 310,00 €')
    expect(within(vue.getByRole('navigation')).getAllByRole('link').map((a) => [texte(a), a.getAttribute('href'), a.getAttribute('aria-current')])).toEqual([
      ['Catégories', '#/tirelire/categories', 'page'], ['Règles', '#/tirelire/categories/regles', null], ['Commerçants', '#/tirelire/categories/commercants', null],
    ])
  })

  it('modifie le nom, la nature et le plafond ; refuse un plafond illisible ; crée et archive', async () => {
    const modifier = vi.spyOn(tirelireApi, 'modifierCategorie').mockImplementation(async (id, c) => { categories = categories.map((x) => (x.id === id ? { ...x, ...(c as Partial<Categorie>) } : x)); return categories.find((x) => x.id === id)! })
    const creer = vi.spyOn(tirelireApi, 'creerCategorie').mockImplementation(async (c) => { const n = categorie({ id: 20, ...c, color: 11 }); categories = [...categories, n]; return n })
    const vue = render(TirelireCategories, { volet: 'categories' })
    await fireEvent.click(await vue.findByRole('button', { name: 'Modifier Courses' }))
    const form = within(vue.getByRole('form', { name: 'Modifier Courses' }))
    expect((form.getByLabelText('Plafond mensuel (€)') as HTMLInputElement).value).toBe('380')
    await fireEvent.input(form.getByLabelText('Plafond mensuel (€)'), { target: { value: 'beaucoup' } })
    expect(texte(form.getByRole('alert'))).toContain('Plafond : un montant en euros')
    expect(form.getByRole('button', { name: 'Enregistrer' })).toBeDisabled()
    await fireEvent.input(form.getByLabelText('Plafond mensuel (€)'), { target: { value: '412,50' } })
    await fireEvent.input(form.getByLabelText('Nom'), { target: { value: ' Alimentation ' } })
    await fireEvent.change(form.getByLabelText('Nature'), { target: { value: 'fixe' } })
    await fireEvent.click(form.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(modifier).toHaveBeenCalledWith(2, { name: 'Alimentation', kind: 'fixe', monthly_cap: 41_250 }))
    await waitFor(() => expect(vue.getAllByRole('listitem').map(texte)).toContain('Alimentation fixe 412,50 €'))

    await fireEvent.click(vue.getByRole('button', { name: 'Nouvelle catégorie' }))
    const nouvelle = within(vue.getByRole('form', { name: 'Nouvelle catégorie' }))
    await fireEvent.input(nouvelle.getByLabelText('Nom'), { target: { value: 'Animaux' } })
    await fireEvent.click(nouvelle.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(creer).toHaveBeenCalledWith({ name: 'Animaux', kind: 'variable', monthly_cap: null }))
    await waitFor(() => expect(vue.getAllByRole('listitem').map(texte)).toContain('Animaux variable —'))
    await vue.findByRole('button', { name: 'Nouvelle catégorie' })

    await fireEvent.click(vue.getByRole('button', { name: 'Modifier Restaurants' }))
    await fireEvent.click(vue.getByRole('button', { name: 'Archiver' }))
    await confirmerLaBoite()
    await waitFor(() => expect(modifier).toHaveBeenLastCalledWith(3, { archived: true }))
    await waitFor(() => expect(vue.getAllByRole('listitem').map(texte)).toContain('Restaurants archivée 150,00 €'))
  })

  it('règles : liste ordonnée, modification des mots-clés, priorité, suppression', async () => {
    let regles = [regle({ id: 1, keywords: ['MARCHE COLLINE', 'MCHE COLLINE'], operations: 104 }), regle({ id: 2, keywords: [], type: 'echeance_pret', category_id: 1, key: 'type-echeance-pret', position: 1, operations: 12 })]
    vi.spyOn(tirelireApi, 'regles').mockImplementation(async () => regles)
    const modifier = vi.spyOn(tirelireApi, 'modifierRegle').mockResolvedValue({ rule: regle(), reclassified: 5 })
    const creer = vi.spyOn(tirelireApi, 'creerRegle').mockResolvedValue({ rule: regle(), reclassified: 0 })
    const ordonner = vi.spyOn(tirelireApi, 'ordonnerRegles').mockImplementation(async (ids) => { regles = ids.map((id) => regles.find((r) => r.id === id)!); return { rules: regles, reclassified: 1 } })
    const supprimer = vi.spyOn(tirelireApi, 'supprimerRegle').mockResolvedValue({ reclassified: 12 })
    const vue = render(TirelireCategories, { volet: 'regles' })
    const liste = () => within(vue.getByRole('list', { name: 'Règles, par ordre de priorité' })).getAllByRole('listitem').map(texte)
    await vue.findByRole('list', { name: 'Règles, par ordre de priorité' })
    expect(liste()).toEqual(['MARCHE COLLINEMCHE COLLINE →Courses· 104 opérations', 'type « échéance de prêt » →Logement· 12 opérations'])
    expect(vue.getByRole('button', { name: 'Monter la règle 1' })).toBeDisabled()

    await fireEvent.click(vue.getByRole('button', { name: 'Modifier la règle 1' }))
    const form = within(vue.getByRole('form', { name: 'Modifier la règle' }))
    await fireEvent.click(form.getByRole('button', { name: 'Retirer MCHE COLLINE' }))
    await fireEvent.input(form.getByLabelText('Ajouter un mot-clé'), { target: { value: ' épicerie du canal ' } })
    await fireEvent.click(form.getByRole('button', { name: 'Ajouter' }))
    await fireEvent.change(form.getByRole('combobox', { name: 'Catégorie de la règle' }), { target: { value: '3' } })
    await fireEvent.click(form.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(modifier).toHaveBeenCalledWith(1, { keywords: ['MARCHE COLLINE', 'EPICERIE DU CANAL'], category_id: 3 }))
    expect(texte(await vue.findByRole('status'))).toBe('Règle enregistrée. 5 opérations reclassées.')

    await fireEvent.click(vue.getByRole('button', { name: 'Descendre la règle 1' }))
    await waitFor(() => expect(ordonner).toHaveBeenCalledWith([2, 1]))
    await waitFor(() => expect(liste()[0]).toContain('échéance de prêt'))

    await fireEvent.click(vue.getByRole('button', { name: 'Nouvelle règle' }))
    const nouvelle = within(vue.getByRole('form', { name: 'Nouvelle règle' }))
    expect(nouvelle.getByRole('button', { name: 'Enregistrer' })).toBeDisabled()
    await fireEvent.input(nouvelle.getByLabelText('Ajouter un mot-clé'), { target: { value: 'livreo' } })
    await fireEvent.change(nouvelle.getByRole('combobox', { name: 'Catégorie de la règle' }), { target: { value: '3' } })
    await fireEvent.click(nouvelle.getByRole('button', { name: 'Enregistrer' }))
    await waitFor(() => expect(creer).toHaveBeenCalledWith({ keywords: ['LIVREO'], category_id: 3 }))
    await waitFor(() => expect(texte(vue.getByRole('status'))).toBe('Règle enregistrée. Aucune opération ne change de catégorie.'))

    await fireEvent.click(vue.getByRole('button', { name: 'Modifier la règle 1' }))
    expect(vue.getByText('Condition fixe : type « échéance de prêt ».')).toBeInTheDocument()
    await fireEvent.click(vue.getByRole('button', { name: 'Supprimer' }))
    await confirmerLaBoite()
    await waitFor(() => expect(supprimer).toHaveBeenCalledWith(2))
    await waitFor(() => expect(texte(vue.getByRole('status'))).toBe('Règle supprimée. 12 opérations reclassées.'))
  })

  it('commerçants : regroupe des variantes, puis en dissocie une ; classe tout le commerçant', async () => {
    let liste: Commercant[] = [commercant({ id: 1, name: 'Marche Colline Villeneuve', operations: 9 }), commercant({ id: 2, name: 'Marche Colline Saint Eloi', operations: 3 }), commercant({ id: 3, name: 'Cafe Tamis Villeneuve', operations: 2, category_id: 3 })]
    let variantes = [{ key: 'MARCHE COLLINE VILLENEUVE', manual: false, operations: 9 }]
    vi.spyOn(tirelireApi, 'commercants').mockImplementation(async () => liste)
    vi.spyOn(tirelireApi, 'commercant').mockImplementation(async (id) => ({ ...liste.find((c) => c.id === id)!, variant_list: variantes }))
    const fusionner = vi.spyOn(tirelireApi, 'fusionner').mockImplementation(async () => {
      liste = [commercant({ id: 1, name: 'Marche Colline Villeneuve', operations: 12, variants: 2 }), liste[2]]
      variantes = [...variantes, { key: 'MARCHE COLLINE SAINT ELOI', manual: true, operations: 3 }]
      return { merchant: liste[0], reclassified: 0 }
    })
    const separer = vi.spyOn(tirelireApi, 'separer').mockImplementation(async () => { variantes = variantes.slice(0, 1); return { merchant: liste[0], created: [] } })
    const classer = vi.spyOn(tirelireApi, 'classerCommercant').mockResolvedValue({ merchant: liste[0], reclassified: 12 })
    const vue = render(TirelireCategories, { volet: 'commercants' })
    const lignes = () => within(vue.getByRole('list', { name: 'Liste des commerçants' })).getAllByRole('button', { expanded: false }).map(texte)
    await vue.findByRole('list', { name: 'Liste des commerçants' })
    expect(lignes()).toEqual(['Marche Colline Villeneuveà classer9 opérations', 'Marche Colline Saint Eloià classer3 opérations', 'Cafe Tamis VilleneuveRestaurants2 opérations'])
    await fireEvent.click(vue.getByRole('checkbox', { name: 'Sélectionner Marche Colline Saint Eloi' }))
    expect(vue.getByRole('button', { name: 'Coche un autre commerçant' })).toBeDisabled()
    await fireEvent.click(vue.getByRole('checkbox', { name: 'Sélectionner Marche Colline Villeneuve' }))
    await fireEvent.click(vue.getByRole('button', { name: 'Regrouper 2 commerçants' }))
    await confirmerLaBoite()
    await waitFor(() => expect(fusionner).toHaveBeenCalledWith(1, [2]))
    expect(texte(await vue.findByRole('status'))).toBe('2 commerçants regroupés sous « Marche Colline Villeneuve ».')
    await waitFor(() => expect(vue.getByText('2 variantes regroupées · 12 opérations')).toBeInTheDocument())

    await fireEvent.change(vue.getByRole('combobox', { name: 'Catégorie de Marche Colline Villeneuve' }), { target: { value: '2' } })
    await waitFor(() => expect(classer).toHaveBeenCalledWith(1, 2))
    await waitFor(() => expect(texte(vue.getByRole('status'))).toBe('12 opérations reclassées.'))

    await fireEvent.click(vue.getByRole('button', { name: 'Dissocier MARCHE COLLINE SAINT ELOI' }))
    await waitFor(() => expect(separer).toHaveBeenCalledWith(1, ['MARCHE COLLINE SAINT ELOI']))
    await waitFor(() => expect(texte(vue.getByRole('status'))).toBe('« MARCHE COLLINE SAINT ELOI » est de nouveau un commerçant à part.'))
    await waitFor(() => expect(vue.queryByRole('button', { name: /Dissocier/ })).toBeNull())
  })
})
