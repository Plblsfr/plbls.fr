import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { categorie, CATEGORIES, detailStats, ligneStats, moisStats, periodeStats, releve } from './essais'
import TirelireSpace from './TirelireSpace.svelte'
import { CLE_MODE } from './stats'
import { tirelireApi } from './tirelire'
import { EtatTirelire, tirelire } from './tirelire.svelte'

const serre = (t: string) => t.replace(/[\s\u00a0\u202f]+/g, '')
const texte = (el: Element | null) => serre(el?.textContent ?? '')
const monter = (section?: string, id?: string, requete?: string) => render(TirelireSpace, { section, id, requete, onmenu: () => {} })

beforeEach(() => {
  localStorage.clear()
  tirelire.reinitialiser()
  tirelire.mode = 'calendar'
  vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve()])
  vi.spyOn(tirelireApi, 'categories').mockResolvedValue(CATEGORIES)
  vi.spyOn(tirelireApi, 'compteur').mockResolvedValue({ operations: 7, merchants: 5, debits: 2800, credits: 0 })
  vi.spyOn(tirelireApi, 'periodes').mockResolvedValue([])
})
afterEach(() => { vi.restoreAllMocks(); tirelire.reinitialiser(); tirelire.mode = 'calendar' })

describe('accueil', () => {
  it('période la plus récente : bandeau, anneau de budget, projection, tuiles et dépassements classés', async () => {
    const lire = vi.spyOn(tirelireApi, 'mois').mockResolvedValue(moisStats())
    const vue = monter()
    expect(await vue.findByRole('heading', { name: 'Novembre 2026', level: 1 })).toBeInTheDocument()
    expect(lire).toHaveBeenCalledWith('calendar', undefined)
    expect(vue.getByText('Jour 26 sur 30 · données jusqu’au 26 nov. 2026')).toBeInTheDocument()
    expect(vue.getByRole('link', { name: /^7 opérations à classer/ })).toHaveAttribute('href', '#/tirelire/a-classer')
    const budget = within(vue.getByRole('region', { name: 'Budget du mois' }))
    expect(budget.getByRole('img', { name: /108\s%\sdu budget consommé/ })).toBeInTheDocument()
    expect(texte(budget.getByText(/au-dessus/))).toBe(serre('+163,40 € au-dessus'))
    expect(texte(budget.getByText(/sans plafond/))).toBe(serre('Dont 28,00 € dans des catégories sans plafond ou à classer.'))
    const projection = within(budget.getByRole('group', { name: 'Projection de fin de mois' }))
    expect(texte(projection.getByText(/≈/))).toBe(serre('≈ 2 430 €'))
    expect(texte(projection.getByText(/vs budget/))).toBe(serre('▲+380 € vs budget à ce rythme'))
    expect(texte(projection.getByText(/Au rythme/))).toContain(serre('environ 54 € par jour'))
    const tuiles = within(vue.getByRole('region', { name: 'Revenus, épargne et reste à vivre' }))
    expect(texte(tuiles.getByText('Revenus').closest('.tir-carte'))).toBe(serre('Revenus2 840,00 €▲+5 %vs octobre'))
    expect(texte(tuiles.getByText('Épargne').closest('.tir-carte'))).toBe(serre('Épargne250,00 €▼−17 %vs octobre'))
    expect(texte(tuiles.getByText('Taux d’épargne').closest('.tir-carte'))).toBe(serre('Taux d’épargne8,8 %épargne ÷ revenus'))
    const depasse = within(vue.getByRole('region', { name: 'Ce qui dépasse' }))
    expect(texte(depasse.getByText(/catégories ·/))).toBe(serre('3 catégories · +132,60 €'))
    const noms = () => depasse.getAllByRole('listitem').map((li) => texte(li.querySelector('.tir-h3')))
    expect(noms()).toEqual(['1.Restaurants', '2.Courses', '3.Santé'])
    expect(texte(depasse.getAllByRole('listitem')[0])).toBe(serre('11. Restaurants236,80 € · plafond 150,00 €▲+86,80 €+58 %'))
    expect(texte(depasse.getAllByRole('listitem')[2])).toContain(serre('moyenne 33,00 € sur 2 mois'))
    expect(depasse.getAllByRole('listitem')[0].querySelector('a')).toHaveAttribute('href', '#/tirelire/categorie/3?periode=2026-11')
    await fireEvent.click(depasse.getByRole('button', { name: 'Classé en %' }))
    expect(noms()).toEqual(['1.Restaurants', '2.Santé', '3.Courses'])
    expect(depasse.getByRole('button', { name: 'Classé en %' })).toHaveAttribute('aria-pressed', 'true')
    expect(texte(depasse.getByText('Sous le plafond :').parentElement)).toBe(serre('Sous le plafond :Transports −17,70 €'))
    expect(depasse.getByRole('link', { name: /Voir le mois en détail/ })).toHaveAttribute('href', '#/tirelire/mois/2026-11')
  })

  it('aucun plafond : pas d’anneau, invitation à en fixer ; rien à comparer avec une seule période', async () => {
    vi.spyOn(tirelireApi, 'mois').mockResolvedValue(moisStats({
      periods: 1, previous_id: null, budget: null, overruns: [], overrun_total: 0, projection: null, projection_reason: 'too_early',
      categories: [ligneStats({ monthly_cap: null, cap_delta: null, cap_bp: null, previous: null, avg3: null, avg12: null, trend: [23_680] })],
      expenses: { ...moisStats().expenses, previous: null, previous_to_date: null, avg3: null, avg12: null },
      income: { ...moisStats().income, previous: null }, savings: { total: 0, rate_bp: null, previous: null }, remaining: { total: 0, previous: null },
      cumulative: { current: [100, 200], previous: null, average: null }, history: moisStats().history.slice(-1),
    }))
    const vue = monter()
    const budget = within(await vue.findByRole('region', { name: 'Budget du mois' }))
    expect(budget.queryByRole('img')).toBeNull()
    expect(budget.getByRole('link', { name: /Fixe des plafonds/ })).toHaveAttribute('href', '#/tirelire/categories')
    expect(texte(budget.getByText(/Pas encore de projection/))).toBe(serre('Pas encore de projection : il faut au moins 5 jours de données sur le mois.'))
    const depasse = within(vue.getByRole('region', { name: 'Ce qui dépasse' }))
    expect(texte(depasse.getByText(/Rien à comparer/))).toContain(serre('aucune catégorie n’a de plafond et il n’y a pas encore de période complète'))
    expect(depasse.queryByRole('button', { name: 'Classé en %' })).toBeNull()
    const tuiles = within(vue.getByRole('region', { name: 'Revenus, épargne et reste à vivre' }))
    expect(tuiles.getAllByText('pas de période précédente')).toHaveLength(3)
    expect(texte(tuiles.getByText('Taux d’épargne').closest('.tir-carte'))).toBe(serre('Taux d’épargne—aucun revenu sur la période'))
    expect(vue.container.querySelector('.tir-etincelle')).toBeNull()
  })

  it('statistiques indisponibles : message et relance', async () => {
    const lire = vi.spyOn(tirelireApi, 'mois').mockRejectedValueOnce(new ApiError(503, 'aegis_unavailable', 'indisponible')).mockResolvedValue(moisStats())
    const vue = monter()
    expect(texte(await vue.findByRole('alert'))).toContain(serre('Statistiques indisponibles'))
    await fireEvent.click(vue.getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('region', { name: 'Ce qui dépasse' })).toBeInTheDocument()
    expect(lire).toHaveBeenCalledTimes(2)
  })
})

describe('vue du mois', () => {
  it('sélecteur, écarts, fixe et variable, courbe avec tableau de données et tableau par catégorie', async () => {
    const lire = vi.spyOn(tirelireApi, 'mois').mockResolvedValue(moisStats({ period: periodeStats({ id: '2026-11' }), next_id: null }))
    const vue = monter('mois', '2026-11')
    const periode = within(await vue.findByRole('region', { name: 'Période' }))
    expect(lire).toHaveBeenCalledWith('calendar', '2026-11')
    expect(periode.getByRole('link', { name: 'Période précédente' })).toHaveAttribute('href', '#/tirelire/mois/2026-10')
    expect(periode.queryByRole('link', { name: 'Période suivante' })).toBeNull()
    expect(periode.getByRole('button', { name: 'Mois calendaire' })).toHaveAttribute('aria-pressed', 'true')
    const depenses = within(vue.getByRole('region', { name: 'Dépenses de la période' }))
    expect(texte(depenses.getByText(/vs octobre/))).toBe(serre('▲+186,40 € vs octobre'))
    expect(texte(depenses.getByText(/Au même jour/))).toBe(serre('Au même jour en octobre : 1 785,00 €, soit ▲+428,40 € à date.'))
    const moyennes = within(vue.getByRole('region', { name: 'Écart avec les moyennes' }))
    expect(texte(moyennes.getByText('vs moyenne 12 mois').closest('.tir-carte'))).toBe(serre('vs moyenne 12 mois▲+14 %1 942 € · moyenne sur 11 mois'))
    const parts = within(vue.getByRole('region', { name: 'Fixe et variable' }))
    expect(parts.getByRole('img').getAttribute('aria-label')?.replace(/[\u00a0\u202f]/g, ' ')).toBe('Fixe : 995,25 €, 45 % ; Variable : 1 190,15 €, 54 % ; À classer : 28,00 €, 1 %')
    const courbe = within(vue.getByRole('region', { name: 'Dépenses cumulées' }))
    expect(texte(courbe.getByRole('list', { name: 'Légende' }))).toBe(serre('Novembre · 2 213 €Octobre · 2 027 €Moyenne 11 mois · 1 942 €Budget · 2 050 €Projection · 2 430 €'))
    expect(courbe.getByRole('slider', { name: 'Parcourir les jours de la courbe' })).toHaveAttribute('aria-valuemax', '30')
    expect(texte(courbe.getByText(/jour par jour, novembre/))).toContain(serre('2 213,00 € au jour 26 sur 30 ; octobre : 2 027,00 € ; moyenne de 11 mois : 1 942,00 € ; budget : 2 050,00 € ; projection : 2 430,00 €.'))
    const donnees = within(courbe.getByRole('table'))
    expect(donnees.getAllByRole('row')).toHaveLength(31)
    expect(texte(donnees.getAllByRole('row')[30])).toBe(serre('30 nov. 2026—2 027,00 €1 942,00 €'))
    const table = within(within(vue.getByRole('region', { name: 'Par catégorie' })).getByRole('table'))
    const lignes = table.getAllByRole('row').map(texte)
    expect(lignes[0]).toBe(serre('CatégorieMontantPartvs octobrevs moy. 3 m.vs moy. 12 m.vs plafond'))
    expect(lignes[1]).toBe(serre('Logementfixe780,00 €35,2 %=0 %=0 %▲+1 %=0,00 €'))
    expect(lignes[3]).toBe(serre('Restaurants236,80 €10,7 %▲+60 %▲+37 %▲+70 %▲+86,80 €'))
    expect(lignes[4]).toBe(serre('Transports142,30 €6,4 %▼−6 %▼−4 %▼−7 %▼−17,70 €'))
    expect(lignes[5]).toBe(serre('Santé46,20 €2,1 %▲+22 %pas de comparaison—pas de comparaison—pas de plafond—'))
    expect(lignes[6]).toBe(serre('À classer7 opérations28,00 €1,3 %▲+28,00 €'))
    expect(lignes[7]).toBe(serre('Total2 213,40 €100 %▲+9 %▲+8 %▲+14 %▲+163,40 €'))
    expect(table.getByRole('link', { name: /Restaurants/ })).toHaveAttribute('href', '#/tirelire/categorie/3?periode=2026-11')
    expect(table.getByRole('link', { name: /À classer/ })).toHaveAttribute('href', '#/tirelire/a-classer')
    expect(within(vue.getByRole('region', { name: 'Par catégorie' })).getByRole('link', { name: 'Opérations' })).toHaveAttribute('href', '#/tirelire/operations?from=2026-11-01&to=2026-11-30&date=purchase')
    expect(within(vue.getByRole('navigation', { name: 'Sections de Tirelire' })).getByRole('link', { name: 'Mois' })).toHaveAttribute('aria-current', 'page')
  })

  it('bascule par relevé : mémorisée, la période la plus récente est rechargée', async () => {
    const lire = vi.spyOn(tirelireApi, 'mois').mockImplementation(async (mode) => (mode === 'statement' ? moisStats({ mode, period: periodeStats({ id: '2026-11-26', from: '2026-10-31', to: '2026-11-26', days: 26, open: false, complete: true, known_through: '2026-11-26' }), previous_id: null, projection: null, projection_reason: 'complete' }) : moisStats()))
    const vue = monter('mois')
    const periode = within(await vue.findByRole('region', { name: 'Période' }))
    await fireEvent.click(periode.getByRole('button', { name: 'Par relevé' }))
    await waitFor(() => expect(lire).toHaveBeenLastCalledWith('statement', undefined))
    expect(localStorage.getItem(CLE_MODE)).toBe('statement')
    expect(await vue.findByText('Relevé du 26 nov. 2026')).toBeInTheDocument()
    expect(texte(vue.getByText(/26 jours/))).toBe(serre('Du 31 oct. au 26 nov. 2026 · 26 jours'))
    expect(vue.getByText('Dépenses du relevé')).toBeInTheDocument()
    expect(new EtatTirelire().mode).toBe('statement')
  })

  it('une seule période, rien de classé : le dire, sans écart ni moyenne', async () => {
    vi.spyOn(tirelireApi, 'mois').mockResolvedValue(moisStats({
      periods: 1, previous_id: null, budget: null, overruns: [], categories: [], projection: null, projection_reason: 'complete', period: periodeStats({ open: false, complete: true, known_through: '2026-11-30', elapsed_days: 30 }),
      expenses: { total: 52_000, fixed: 0, variable: 0, unclassified: 52_000, fixed_bp: 0, variable_bp: 0, unclassified_bp: 10_000, previous: null, previous_to_date: null, avg3: null, avg12: null },
      unclassified: { amount: 52_000, operations: 31, share_bp: 10_000, previous: null }, cumulative: { current: Array.from({ length: 30 }, (_, i) => i * 1000), previous: null, average: null },
    }))
    const vue = monter('mois')
    const depenses = within(await vue.findByRole('region', { name: 'Dépenses de la période' }))
    expect(texte(depenses.getByText(/Une seule période/))).toBe(serre('Une seule période disponible : pas encore d’écart ni de moyenne. Ils apparaîtront avec un deuxième mois.'))
    expect(vue.queryByRole('region', { name: 'Écart avec les moyennes' })).toBeNull()
    const categories = within(vue.getByRole('region', { name: 'Par catégorie' }))
    expect(texte(categories.getByText(/Aucune opération de cette période/))).toContain(serre('tout est dans « À classer »'))
    expect(categories.getAllByRole('row').map(texte)[1]).toBe(serre('À classer31 opérations520,00 €100,0 %pas de comparaison—'))
    expect(categories.getByRole('link', { name: /Aucun plafond fixé/ })).toHaveAttribute('href', '#/tirelire/categories')
    expect(texte(within(vue.getByRole('region', { name: 'Dépenses cumulées' })).getByRole('list', { name: 'Légende' }))).toBe(serre('Novembre · 290 €'))
  })

  it('période inconnue : message et retour à la plus récente', async () => {
    vi.spyOn(tirelireApi, 'mois').mockRejectedValue(new ApiError(404, 'not_found', 'introuvable'))
    const vue = monter('mois', '2020-01')
    expect(await vue.findByRole('heading', { name: 'Période introuvable' })).toBeInTheDocument()
    expect(vue.getByRole('link', { name: 'Voir la période la plus récente' })).toHaveAttribute('href', '#/tirelire/mois')
  })
})

describe('détail d’une catégorie', () => {
  it('montant, historique avec plafond, commerçants et opérations liées à la liste filtrée', async () => {
    const lire = vi.spyOn(tirelireApi, 'categorieDuMois').mockResolvedValue(detailStats())
    const vue = monter('categorie', '3', 'periode=2026-11')
    expect(await vue.findByRole('heading', { name: 'Restaurants', level: 1 })).toBeInTheDocument()
    expect(lire).toHaveBeenCalledWith(3, 'calendar', '2026-11')
    expect(vue.getByText('dépense variable · novembre 2026')).toBeInTheDocument()
    expect(vue.getByRole('link', { name: 'Retour à la vue du mois' })).toHaveAttribute('href', '#/tirelire/mois/2026-11')
    const montant = within(vue.getByRole('region', { name: 'Montant de la période' }))
    expect(texte(montant.getByText(/au-dessus du plafond/))).toBe(serre('+58 % au-dessus du plafond'))
    expect(texte(montant.getByText('vs octobre').parentElement)).toBe(serre('vs octobre▲+60 %'))
    const historique = within(vue.getByRole('region', { name: 'Historique' }))
    expect(historique.getByRole('img').getAttribute('aria-label')).toMatch(/Novembre 2026 236,80\s€\. Plafond 150,00\s€\.$/)
    expect(texte(historique.getByText(/Plafond dépassé/))).toBe(serre('Plafond dépassé 2 fois sur 12 mois. Les périodes hachurées sont incomplètes.'))
    expect(within(historique.getByRole('table')).getAllByRole('row')).toHaveLength(13)
    const commercants = within(vue.getByRole('region', { name: 'Commerçants' })).getAllByRole('listitem')
    expect(texte(commercants[0])).toBe(serre('Livreo118,40 €6 opérations · panier moyen 19,73 €▲+92 %'))
    expect(commercants[0].querySelector('a')).toHaveAttribute('href', '#/tirelire/operations?from=2026-11-01&to=2026-11-30&category=3&merchant=21&date=purchase')
    expect(texte(commercants[2])).toBe(serre('Sans commerçant43,90 €11 opérations · panier moyen 3,99 €· absent en octobre'))
    expect(commercants[2].querySelector('a')).toBeNull()
    const operations = within(vue.getByRole('region', { name: 'Opérations' }))
    expect(operations.getAllByRole('listitem').map(texte)).toEqual(['22nov.Livreo−41,80€', '22nov.BistrotLune−28,50€'])
    expect(operations.getByRole('link', { name: 'Voir les 20 opérations' })).toHaveAttribute('href', '#/tirelire/operations?from=2026-11-01&to=2026-11-30&category=3&date=purchase')
  })

  it('plafond modifiable sur place : boutons, repères, enregistrement puis rechargement', async () => {
    let plafond: number | null = 15_000
    const lire = vi.spyOn(tirelireApi, 'categorieDuMois').mockImplementation(async () => detailStats({ category: ligneStats({ monthly_cap: plafond }) }))
    const modifier = vi.spyOn(tirelireApi, 'modifierCategorie').mockImplementation(async (_id, c) => { plafond = c.monthly_cap ?? null; return categorie({ id: 3, monthly_cap: plafond }) })
    const vue = monter('categorie', '3', 'periode=2026-11')
    const form = within(await vue.findByRole('form', { name: 'Plafond mensuel' }))
    const champ = form.getByLabelText('Plafond mensuel en euros') as HTMLInputElement
    expect(champ.value).toBe('150')
    expect(form.getByRole('button', { name: 'Enregistrer le plafond' })).toBeDisabled()
    await fireEvent.click(form.getByRole('button', { name: 'Monter le plafond' }))
    expect(champ.value).toBe('155')
    await fireEvent.click(form.getByRole('button', { name: /moy\. 3 mois/ }))
    expect(champ.value).toBe('173')
    await fireEvent.input(champ, { target: { value: 'beaucoup' } })
    expect(texte(form.getByRole('alert'))).toContain(serre('un montant en euros'))
    expect(form.getByRole('button', { name: 'Enregistrer le plafond' })).toBeDisabled()
    await fireEvent.input(champ, { target: { value: '180' } })
    await fireEvent.click(form.getByRole('button', { name: 'Enregistrer le plafond' }))
    await waitFor(() => expect(modifier).toHaveBeenCalledWith(3, { monthly_cap: 18_000 }))
    expect(texte(await form.findByRole('status'))).toBe(serre('Plafond enregistré : 180,00 € par mois.'))
    await waitFor(() => expect(lire).toHaveBeenCalledTimes(2))
    await fireEvent.click(form.getByRole('button', { name: 'Retirer' }))
    await waitFor(() => expect(modifier).toHaveBeenLastCalledWith(3, { monthly_cap: null }))
    await waitFor(() => expect(champ.value).toBe(''))
  })

  it('catégorie inconnue : message', async () => {
    vi.spyOn(tirelireApi, 'categorieDuMois').mockRejectedValue(new ApiError(404, 'not_found', 'introuvable'))
    const vue = monter('categorie', '99')
    expect(await vue.findByRole('heading', { name: 'Catégorie ou période introuvable' })).toBeInTheDocument()
  })
})
