import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { anneesStats, CATEGORIES, historiqueStats, ligneHistorique, releve, soldesStats, tendanceStats } from './essais'
import { CLE_FENETRE } from './evolution'
import TirelireSpace from './TirelireSpace.svelte'
import { CLE_MODE } from './stats'
import { tirelireApi } from './tirelire'
import { tirelire } from './tirelire.svelte'

const serre = (t: string) => t.replace(/[\s  ]+/g, '')
const texte = (el: Element | null) => serre(el?.textContent ?? '')
const monter = () => render(TirelireSpace, { section: 'evolution', onmenu: () => {} })

function simuler(o: { historique?: ReturnType<typeof historiqueStats>; soldes?: ReturnType<typeof soldesStats>; annees?: ReturnType<typeof anneesStats> | null } = {}) {
  return {
    historique: vi.spyOn(tirelireApi, 'historique').mockResolvedValue(o.historique ?? historiqueStats()),
    soldes: vi.spyOn(tirelireApi, 'soldes').mockResolvedValue(o.soldes ?? soldesStats()),
    annees: o.annees === null ? vi.spyOn(tirelireApi, 'annees').mockRejectedValue(new ApiError(404, 'not_found', 'introuvable')) : vi.spyOn(tirelireApi, 'annees').mockResolvedValue(o.annees ?? anneesStats()),
  }
}

beforeEach(() => {
  localStorage.clear()
  tirelire.reinitialiser()
  tirelire.mode = 'calendar'
  tirelire.fenetre = '12'
  vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve()])
  vi.spyOn(tirelireApi, 'categories').mockResolvedValue(CATEGORIES)
  vi.spyOn(tirelireApi, 'compteur').mockResolvedValue({ operations: 0, merchants: 0, debits: 0, credits: 0 })
})
afterEach(() => { vi.restoreAllMocks(); tirelire.reinitialiser(); tirelire.mode = 'calendar'; tirelire.fenetre = '12' })

describe('écran Évolution', () => {
  it('graphique empilé : légende, une barre par mois, lecture de la période visée', async () => {
    const api = simuler()
    const vue = monter()
    const graphe = within(await vue.findByRole('region', { name: 'Dépenses par catégorie' }))
    expect(api.historique).toHaveBeenCalledWith('calendar', '12')
    expect(api.soldes).toHaveBeenCalledWith('calendar', '12')
    expect(api.annees).toHaveBeenCalledWith('calendar')
    expect(vue.getByRole('heading', { name: 'Évolution', level: 1 })).toBeInTheDocument()
    expect(within(graphe.getByRole('list', { name: 'Légende' })).getAllByRole('listitem').map(texte)).toEqual(['Logement', 'Courses', 'Restaurants', 'Transports', 'Autres(1)', 'Àclasser', serre('Budget · 1 310 €'), 'Moisincomplet'])
    const barres = graphe.getByRole('slider', { name: 'Parcourir les mois du graphique' })
    expect(barres.querySelectorAll('.tir-emp-col')).toHaveLength(6)
    expect(barres).toHaveAttribute('aria-valuenow', '6')
    expect(barres.getAttribute('aria-valuetext')).toContain('novembre 2026 (incomplet)')
    const lecture = within(graphe.getByRole('status'))
    expect(texte(lecture.getByText(/Novembre 2026/))).toBe(serre('Novembre 2026 · en cours'))
    expect(lecture.getByRole('link', { name: /Voir le mois/ })).toHaveAttribute('href', '#/tirelire/mois/2026-11')
    expect(lecture.getAllByRole('listitem').map(texte)).toContain(serre('À classer28,00 €'))
    await fireEvent.keyDown(barres, { key: 'Home' })
    expect(serre(barres.getAttribute('aria-valuetext') ?? '')).toBe(serre('juin 2026 : 1 448,00 € ; Logement 780,00 €, Courses 352,00 €, Restaurants 128,00 €, Transports 158,00 €, Autres (1) 30,00 €'))
    await fireEvent.keyDown(barres, { key: 'ArrowRight' })
    expect(barres).toHaveAttribute('aria-valuenow', '2')
    expect(texte(within(graphe.getByRole('status')).getByText(/Juillet 2026/))).toBe('Juillet2026')
    expect(within(graphe.getByRole('status')).getByRole('link', { name: /Voir le mois/ })).toHaveAttribute('href', '#/tirelire/mois/2026-07')
  })

  it('tableau catégories × mois : montants, total, moyenne, plafond et dépassements signalés autrement que par la couleur', async () => {
    simuler()
    const vue = monter()
    const carte = within(await vue.findByRole('region', { name: 'Catégories × mois' }))
    const table = carte.getByRole('table')
    expect(within(table).getAllByRole('columnheader').map(texte)).toEqual(['Catégorie', 'juin2026', 'juil.', 'août', 'sept.', 'oct.', 'nov.*', 'Total', 'Moy.', 'Plafond', 'Dépass.:nombredepériodesau-dessusduplafond'])
    const ligne = (nom: RegExp) => within(within(table).getByRole('row', { name: nom })).getAllByRole('cell').map(texte)
    expect(ligne(/^Restaurants/)).toEqual(['128', '▲au-dessusduplafond:155', '139', '133', '148', '▲au-dessusduplafond:237', '940', '141', '150', '2mois/6'])
    expect(ligne(/^Transports/).slice(6)).toEqual(['885', '149', 'pasdeplafond—', 'pasdeplafond—'])
    expect(ligne(/^À classer/)).toEqual(['0', '0', '0', '0', '0', '28', '28', '0', '—', '—'])
    expect(ligne(/^Total/).slice(6)).toEqual(['8981', '1467', '1310', '6mois/6'])
    expect(within(table).getByRole('link', { name: /Restaurants/ })).toHaveAttribute('href', '#/tirelire/categorie/3?periode=2026-11')
    expect(texte(carte.getByText(/^montants au-dessus du plafond$/))).toBe(serre('▲3montants au-dessus du plafond'))
    expect(carte.getByRole('region', { name: /tableau à faire défiler/ })).toHaveAttribute('tabindex', '0')
  })

  it('tendance : résumé, une ligne par catégorie avec mini-courbe, sens et variation chiffrée', async () => {
    simuler()
    const vue = monter()
    const carte = within(await vue.findByRole('region', { name: 'Tendance par catégorie' }))
    expect(carte.getByText('sur les 5 derniers mois complets')).toBeInTheDocument()
    expect(within(carte.getByRole('list', { name: 'Résumé des tendances' })).getAllByRole('listitem').map(texte)).toEqual(['▲1enhausse', '=3stables', '▼1enbaisse'])
    const restaurants = carte.getByRole('link', { name: /Restaurants/ })
    expect(texte(restaurants)).toBe(serre('Restaurants+30 € en 5 mois · moy. 139 €▲hausse +22 %'))
    expect(restaurants.querySelector('svg.tir-etincelle')).not.toBeNull()
    expect(texte(carte.getByRole('link', { name: /Transports/ }))).toContain(serre('▼baisse −11 %'))
    expect(texte(carte.getByRole('link', { name: /Logement/ }))).toContain(serre('=stable 0 %'))
    expect(texte(within(vue.getByRole('region', { name: 'Tendance des dépenses' })).getByText(/stable/))).toBe(serre('=stable +3 %'))
  })

  it('solde : dernier solde, plus bas et plus haut, trous dits et jamais comblés', async () => {
    simuler()
    const vue = monter()
    const carte = within(await vue.findByRole('region', { name: 'Solde du compte' }))
    expect(texte(carte.getByText(/^au /))).toBe(serre('au 26 nov. 2026'))
    const bas = carte.getByText('Solde le plus bas').parentElement!
    expect(texte(bas)).toBe(serre('Solde le plus bas−45,00 €le 5 oct. 2026'))
    expect(texte(carte.getByText('Solde le plus haut').parentElement)).toBe(serre('Solde le plus haut1 900,00 €le 23 nov. 2026'))
    expect(texte(carte.getByText(/sans relevé \(/))).toBe(serre('5 jours sans relevé (du 27 au 31 oct. 2026) : la courbe s’interrompt, rien n’est estimé.'))
    expect(texte(carte.getByText(/entre le/))).toBe(serre('+700,00 € entre le 1er oct. 2026 et le 26 nov. 2026.'))
    const courbe = carte.getByRole('slider', { name: 'Parcourir les jours du solde' })
    expect(courbe.querySelectorAll('path.tir-serie')).toHaveLength(2)
    expect(courbe.querySelectorAll('rect.tir-trou')).toHaveLength(1)
    expect(serre(courbe.getAttribute('aria-valuetext') ?? '')).toBe(serre('26 nov. 2026 : 1 900,00 €'))
    await fireEvent.keyDown(courbe, { key: 'PageDown' })
    expect(serre(courbe.getAttribute('aria-valuetext') ?? '')).toBe(serre('27 oct. 2026 : aucun relevé ne couvre ce jour'))
    expect(texte(courbe.querySelector('.tir-bulle'))).toBe(serre('27 oct. 2026pas de relevé ce jour'))
    const lignes = within(carte.getByRole('table')).getAllByRole('row').map(texte)
    expect(lignes).toContain(serre('Du 27 au 31 oct. 2026pas de relevé'))
    expect(lignes).toContain(serre('5 oct. 2026−45,00 €'))
    expect(carte.queryByText(/à vérifier/)).toBeNull()
  })

  it('année sur l’autre : même mois, cumul sur les mois communs, tableau par catégorie', async () => {
    simuler()
    const vue = monter()
    const carte = within(await vue.findByRole('region', { name: 'D’une année sur l’autre' }))
    expect(carte.getByRole('heading', { name: '2026 vs 2025' })).toBeInTheDocument()
    expect(texte(carte.getByText(/sur 2 mois$/))).toBe(serre('▲+5 % sur 2 mois'))
    expect(texte(carte.getByText('Novembre 2026 vs novembre 2025').parentElement)).toBe(serre('Novembre 2026 vs novembre 20252 213,40 €▲+9 % (+173,40 €) · 2 040 € en 2025'))
    expect(texte(carte.getByText('Cumul janv. → nov. 2026').parentElement)).toBe(serre('Cumul janv. → nov. 202611 920,40 €▲+5 % (+210,40 €) · sur oct., nov. : 4 240 € contre 4 030 €'))
    expect(carte.getByText('Le cumul n’est comparé que sur les mois présents dans les deux années (oct., nov.).')).toBeInTheDocument()
    expect(carte.getByText(/couvert qu’en partie/)).toBeInTheDocument()
    const table = carte.getByRole('table')
    expect(within(table).getAllByRole('columnheader').map(texte)).toEqual(['Catégorie', 'nov.2026', 'nov.2025', 'Écart', 'Cumul2026(moiscomparés)', 'Cumul2025', 'Écart'])
    expect(within(within(table).getByRole('row', { name: /^Restaurants/ })).getAllByRole('cell').map(texte)).toEqual([serre('236,80 €'), serre('0,00 €'), serre('▲+236,80 €'), serre('384,80 €'), serre('0,00 €'), serre('▲+384,80 €')])
    expect(within(within(table).getByRole('row', { name: /^Total/ })).getAllByRole('cell').map(texte)[3]).toBe(serre('4 240,40 €'))
    const courbe = carte.getByRole('slider', { name: 'Parcourir les mois de l’année' })
    expect(serre(courbe.getAttribute('aria-valuetext') ?? '')).toBe(serre('novembre : 2026 2 213,40 €, 2025 2 040,00 €'))
    await fireEvent.keyDown(courbe, { key: 'ArrowRight' })
    expect(serre(courbe.getAttribute('aria-valuetext') ?? '')).toBe(serre('décembre : 2026 aucune donnée, 2025 2 165,00 €'))
  })

  it('fenêtre et mode : rechargent, sont mémorisés, et le mode suit celui de la vue du mois', async () => {
    localStorage.setItem(CLE_MODE, 'statement')
    tirelire.mode = 'statement'
    const api = simuler({ historique: historiqueStats({ mode: 'statement', window: '12' }) })
    const vue = monter()
    const reglages = within(await vue.findByRole('region', { name: 'Fenêtre et découpage' }))
    expect(api.historique).toHaveBeenCalledWith('statement', '12')
    expect(within(reglages.getByRole('group', { name: 'Nombre de périodes affichées' })).getAllByRole('button').map(texte)).toEqual(['6relevés', '12relevés', '24relevés', 'Tout'])
    expect(reglages.getByRole('button', { name: 'Par relevé' })).toHaveAttribute('aria-pressed', 'true')
    expect(reglages.getByRole('button', { name: '12 relevés' })).toHaveAttribute('aria-pressed', 'true')
    api.historique.mockResolvedValue(historiqueStats({ mode: 'statement', window: 'all' }))
    await fireEvent.click(reglages.getByRole('button', { name: 'Tout' }))
    await waitFor(() => expect(api.historique).toHaveBeenLastCalledWith('statement', 'all'))
    expect(api.soldes).toHaveBeenLastCalledWith('statement', 'all')
    expect(localStorage.getItem(CLE_FENETRE)).toBe('all')
    api.historique.mockResolvedValue(historiqueStats({ window: 'all' }))
    await fireEvent.click(within(vue.getByRole('region', { name: 'Fenêtre et découpage' })).getByRole('button', { name: 'Mois calendaire' }))
    await waitFor(() => expect(api.historique).toHaveBeenLastCalledWith('calendar', 'all'))
    expect(localStorage.getItem(CLE_MODE)).toBe('calendar')
    expect(await vue.findByRole('region', { name: 'Catégories × mois' })).toBeInTheDocument()
  })
})

describe('états limites', () => {
  it('une seule période, aucun plafond, pas d’année précédente', async () => {
    const seule = historiqueStats({
      window: '12', available: 1, complete: 0, periods: historiqueStats().periods.slice(5), trend_period_ids: [],
      categories: [ligneHistorique({ monthly_cap: null, amounts: [23_680], total: 23_680, average: null, over_cap: null, trend: tendanceStats({ direction: 'insufficient', periods: 0, average: null, delta: null, delta_bp: null, values: [] }) })],
      unclassified: { amounts: [0], total: 0, average: null },
      total: { amounts: [23_680], total: 23_680, average: null, budget: null, over_budget: null, trend: tendanceStats({ direction: 'insufficient', periods: 0, average: null, delta: null, delta_bp: null, values: [] }) },
      chart: { category_ids: [3], others: [0], others_categories: 0 },
    })
    const annee = anneesStats({ has_previous: false, compared_months: [], partial: false, months: anneesStats().months.map((m) => ({ ...m, previous: null, previous_complete: null })), total: { month: 23_680, month_previous: null, ytd: 23_680, ytd_compared: null }, categories: [{ id: 3, name: 'Restaurants', kind: 'variable', color: 2, month: 23_680, month_previous: null, ytd: 23_680, ytd_compared: null }], unclassified: { month: 0, month_previous: null, ytd: 0, ytd_compared: null } })
    simuler({ historique: seule, annees: annee })
    const vue = monter()
    expect(await vue.findByText('Une seule période pour l’instant')).toBeInTheDocument()
    expect(texte(vue.getByRole('region', { name: 'Moyenne des dépenses' }))).toBe(serre('Moyenne—aucune période complète dans la fenêtre'))
    expect(texte(vue.getByRole('region', { name: 'Périodes au-dessus du budget' }))).toBe(serre('Mois au-dessus du budget—aucun plafond fixé'))
    const table = within(vue.getByRole('region', { name: 'Catégories × mois' }))
    expect(table.getByRole('link', { name: /Aucun plafond fixé/ })).toHaveAttribute('href', '#/tirelire/categories')
    expect(table.queryByText(/^montants? au-dessus du plafond$/)).toBeNull()
    expect(within(table.getByRole('row', { name: /^Restaurants/ })).getAllByRole('cell').map(texte)).toEqual(['237', '237', '—', 'pasdeplafond—', 'pasdeplafond—'])
    const tendance = within(vue.getByRole('region', { name: 'Tendance par catégorie' }))
    expect(tendance.getByText(/Pas assez de données : il faut au moins 3 mois complets/)).toBeInTheDocument()
    expect(tendance.queryByRole('list')).toBeNull()
    const annees = within(vue.getByRole('region', { name: 'D’une année sur l’autre' }))
    expect(annees.getByText('Pas encore de 2025 à comparer')).toBeInTheDocument()
    expect(texte(annees.getByText('Novembre 2026 vs novembre 2025').parentElement)).toBe(serre('Novembre 2026 vs novembre 2025236,80 €pas de novembre 2025 dans tes relevés'))
    expect(within(annees.getByRole('table')).getAllByRole('columnheader').map(texte)).toEqual(['Catégorie', 'nov.2026', 'Cumul2026'])
    expect(annees.getByRole('slider').querySelectorAll('.tir-point--avant')).toHaveLength(0)
  })

  it('solde non rapproché, fenêtre sans jour couvert, aucune période annuelle', async () => {
    const compte = soldesStats().accounts[0]
    simuler({ soldes: soldesStats({ accounts: [{ ...compte, reconciled: false }, { account_id: 2, days: 0, reconciled: true, segments: [], gaps: [{ from: '2026-10-01', to: '2026-11-26', days: 57 }], first: null, last: null, min: null, max: null }] }), annees: null })
    const vue = monter()
    const carte = within(await vue.findByRole('region', { name: 'Solde du compte' }))
    expect(carte.getByText(/ne retombe pas sur le nouveau solde/)).toBeInTheDocument()
    await fireEvent.change(carte.getByRole('combobox', { name: 'Compte affiché' }), { target: { value: '2' } })
    expect(carte.getByText('Aucun relevé ne couvre cette période : pas de solde à montrer.')).toBeInTheDocument()
    expect(within(vue.getByRole('region', { name: 'D’une année sur l’autre' })).getByText('Aucune période à comparer pour l’instant.')).toBeInTheDocument()
  })

  it('service en erreur : message et relance', async () => {
    const api = simuler()
    api.historique.mockRejectedValueOnce(new ApiError(503, 'unavailable', 'indisponible'))
    const vue = monter()
    expect(await vue.findByRole('heading', { name: 'Évolution indisponible' })).toBeInTheDocument()
    await fireEvent.click(vue.getByRole('button', { name: 'Réessayer' }))
    expect(await vue.findByRole('region', { name: 'Dépenses par catégorie' })).toBeInTheDocument()
  })
})
