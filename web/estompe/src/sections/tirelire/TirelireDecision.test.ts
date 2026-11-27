import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest'
import { ApiError } from '../../lib/api'
import { confirmation } from '../../lib/confirm.svelte'
import { simuler, type Reglages } from './simulation'
import { CATEGORIES, moisStats, releve } from './essais'
import { resumePremier, resumeStats, simulationStats } from './essais-decision'
import TirelireAccueil from './TirelireAccueil.svelte'
import TirelireBref from './TirelireBref.svelte'
import TirelireDecision from './TirelireDecision.svelte'
import TirelireMois from './TirelireMois.svelte'
import { tirelireApi } from './tirelire'
import { tirelire } from './tirelire.svelte'

const texte = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''
interface Vue { container: HTMLElement }
const curseur = (vue: Vue, nom: string) => within(vue.container).getByRole('slider', { name: `Baisse de ${nom} en pourcentage` }) as HTMLInputElement
const resultat = (vue: Vue) => texte(within(vue.container).getByRole('region', { name: 'Résultat de la simulation' }))

async function glisser(vue: Vue, nom: string, valeur: number) {
  await fireEvent.input(curseur(vue, nom), { target: { value: String(valeur) } })
  await fireEvent.change(curseur(vue, nom))
}

let service: MockInstance<typeof tirelireApi.simuler>

beforeEach(() => {
  tirelire.reinitialiser()
  localStorage.clear()
  vi.spyOn(tirelireApi, 'categories').mockResolvedValue(CATEGORIES)
  vi.spyOn(tirelireApi, 'compteur').mockResolvedValue({ operations: 0, merchants: 0, debits: 0, credits: 0 })
  vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve()])
  service = vi.spyOn(tirelireApi, 'simuler').mockImplementation(async (_mode, r: Reglages) => simuler(simulationStats(), r))
})
afterEach(() => { confirmation.en?.resoudre({ ok: false, valeur: '' }); vi.restoreAllMocks(); tirelire.reinitialiser() })

describe('TirelireDecision', () => {
  it('résumé, leviers classés avec leurs liens, simulateur au repos', () => {
    const vue = render(TirelireDecision, { resume: resumeStats(), simulation: simulationStats() })
    const bref = vue.getByRole('region', { name: 'Résumé de la période' })
    expect(within(bref).getByRole('heading', { name: 'Novembre en bref' })).toBeInTheDocument()
    expect(texte(bref)).toContain('Au 26 nov. 2026, tu as dépensé 2 213,40 € en novembre, soit 186,40 € de plus qu’en octobre (+9 %) et 163,40 € au-dessus de ton budget.')
    expect(texte(within(bref).getByRole('group', { name: 'Par rapport à octobre' }))).toMatch(/^Par rapport à octobre ?▲ ?en hausse ?Restaurants \(\+88,80 €\) et Courses \(\+41,60 €\)\.$/)
    expect(texte(within(bref).getByRole('group', { name: 'Par rapport à ta moyenne des 3 derniers mois complets' }))).toContain('Restaurants (+63,80 €).')
    expect(texte(bref)).toContain('Taux d’épargne de 8,8 %, contre 11,1 % en octobre.')
    expect(texte(bref)).toContain('La période est en cours : les baisses ne sont pas citées')
    expect(within(bref).getByRole('link', { name: 'Résumé de la période précédente' })).toHaveAttribute('href', '#/tirelire/decision?periode=2026-10')
    const leviers = within(vue.getByRole('region', { name: 'Leviers les plus rentables' }))
    expect(leviers.getByRole('heading', { name: 'Les 3 leviers les plus rentables' })).toBeInTheDocument()
    expect(leviers.getAllByRole('link').map((a) => a.getAttribute('href'))).toEqual(['#/tirelire/decision?cible=3:15000', '#/tirelire/habitudes', '#/tirelire/decision?cible=2:38000'])
    expect(texte(leviers.getAllByRole('listitem')[0])).toMatch(/^1 ?1\. ?Restaurants ?revenir au plafond de 150,00 € \(dépensé : 236,80 €\) ?Simuler ?1 041,60 € ?par an ?86,80 € par mois$/)
    expect(texte(vue.getByRole('region', { name: 'Leviers les plus rentables' }))).toContain('2 autres pistes moins rentables ne sont pas affichées.')
    const simulateur = within(vue.getByRole('region', { name: 'Simulateur' }))
    expect(simulateur.getAllByRole('slider').map((s) => s.getAttribute('aria-label'))).toEqual(['Courses', 'Restaurants', 'Shopping', 'Loisirs et sorties', 'Transports'].map((n) => `Baisse de ${n} en pourcentage`))
    expect(texte(vue.getByRole('region', { name: 'Simulateur' }))).toContain('Base : ta dépense moyenne de chaque catégorie sur tes 3 derniers mois complets (du 1er août au 31 oct. 2026).')
    expect(curseur(vue, 'Courses')).toHaveAttribute('step', '5')
    expect(resultat(vue)).toMatch(/Avec ces réglages, tu économises ?0,00 € ?par mois ?0,00 € ?par an/)
    expect(resultat(vue)).toContain('Déplace un curseur')
    expect(vue.getByRole('button', { name: 'Fixer comme plafond' })).toBeDisabled()
    expect(vue.getByRole('button', { name: 'Réinitialiser' })).toBeDisabled()
    expect(service).not.toHaveBeenCalled()
  })

  it('curseurs, montant cible, catégorie fixe ajoutée, abonnement arrêté, réinitialisation', async () => {
    const vue = render(TirelireDecision, { resume: null, simulation: simulationStats() })
    await fireEvent.input(curseur(vue, 'Restaurants'), { target: { value: '30' } })
    expect(texte(vue.getByRole('group', { name: 'Restaurants' }))).toMatch(/Restaurants ?−30 % ?Dépense habituelle : ?236,80 € ?→ ?Montant cible de Restaurants en euros par mois ?€ ?−71,04 € par mois/)
    expect((vue.getByRole('textbox', { name: 'Montant cible de Restaurants en euros par mois' }) as HTMLInputElement).value).toBe('165,76')
    expect(service).not.toHaveBeenCalled()
    await fireEvent.change(curseur(vue, 'Restaurants'))
    await waitFor(() => expect(service).toHaveBeenLastCalledWith('calendar', { categories: { 3: { percent: 30 } }, arrets: [] }))
    await glisser(vue, 'Loisirs et sorties', 25)
    await glisser(vue, 'Shopping', 20)
    await glisser(vue, 'Courses', 5)
    expect(resultat(vue)).toMatch(/tu économises ?170,37 € ?par mois ?2 044,44 € ?par an/)
    expect(resultat(vue)).toContain('Le trait marque ton budget de 2 050,00 € : tu serais 176,37 € en dessous. Si tu épargnes cette somme, ton taux d’épargne passe de 11,1 % à 17,4 %. Sinon, ton reste à vivre passe de 356,00 € à 526,37 €.')
    expect(within(vue.getByRole('region', { name: 'Résultat de la simulation' })).getByRole('img').getAttribute('aria-label')?.replace(/\s/g, ' ')).toBe('Dépenses par mois : 2 044,00 € d’habitude, 1 873,63 € simulé, pour un budget de 2 050,00 €')
    expect(curseur(vue, 'Courses').getAttribute('aria-valuetext')?.replace(/\s/g, ' ')).toBe('5 %, 20,63 € économisés par mois')

    const cible = vue.getByRole('textbox', { name: 'Montant cible de Transports en euros par mois' }) as HTMLInputElement
    await fireEvent.change(cible, { target: { value: '120,50' } })
    expect(texte(vue.getByRole('group', { name: 'Transports' }))).toContain('−19 %')
    expect(texte(vue.getByRole('group', { name: 'Transports' }))).toContain('−27,50 € par mois')
    await waitFor(() => expect(service).toHaveBeenLastCalledWith('calendar', expect.objectContaining({ categories: expect.objectContaining({ 6: { target: 12_050 } }) })))
    await fireEvent.change(cible, { target: { value: '200' } })
    expect(texte(vue.getByRole('alert'))).toBe('Transports : la cible ne peut pas dépasser ta dépense habituelle (148,00 €).')
    expect(cible.value).toBe('148')
    await fireEvent.change(cible, { target: { value: 'beaucoup' } })
    expect(texte(vue.getByRole('alert'))).toContain('un montant en euros')

    const ajout = vue.getByRole('combobox', { name: 'Ajouter une catégorie au simulateur' }) as HTMLSelectElement
    expect([...ajout.options].map((o) => [texte(o), o.disabled])).toEqual([['Ajouter une catégorie', false], ['Logement (fixe) · 780,00 €', false], ['Abonnements (fixe) · 118,47 €', false], ['Santé · sans historique', true]])
    await fireEvent.change(ajout, { target: { value: '9' } })
    expect(texte(vue.getByRole('group', { name: 'Abonnements' }))).toMatch(/^Abonnements ?fixe ?0 %/)
    await glisser(vue, 'Abonnements', 10)
    await fireEvent.click(vue.getByRole('checkbox', { name: /Streamo/ }))
    expect(texte(vue.getByRole('group', { name: 'Abonnements' }))).toContain('−10,45 € par mois')
    expect(texte(vue.getByRole('group', { name: 'Abonnements' }))).toContain('13,99 € d’abonnements arrêtés sont retirés avant la baisse.')
    expect(texte(vue.getByRole('checkbox', { name: /Streamo/ }).closest('label'))).toMatch(/Streamo ?13,99 € · mensuel ?167,88 € ?par an/)
    expect(resultat(vue)).toMatch(/tu économises ?194,81 € ?par mois ?2 337,72 € ?par an/)
    await waitFor(() => expect(service).toHaveBeenLastCalledWith('calendar', expect.objectContaining({ arrets: ['54-monthly'] })))

    await fireEvent.click(vue.getByRole('button', { name: 'Réinitialiser' }))
    expect(resultat(vue)).toMatch(/tu économises ?0,00 € ?par mois/)
    expect(vue.getByRole('checkbox', { name: /Streamo/ })).not.toBeChecked()
    expect(curseur(vue, 'Restaurants').value).toBe('0')
  })

  it('« Fixer comme plafond » : confirmation, puis un appel par catégorie réglée à la route de plafond', async () => {
    const modifier = vi.spyOn(tirelireApi, 'modifierCategorie').mockResolvedValue(CATEGORIES[2])
    const vue = render(TirelireDecision, { resume: null, simulation: simulationStats() })
    await glisser(vue, 'Restaurants', 30)
    await glisser(vue, 'Shopping', 20)
    await fireEvent.click(vue.getByRole('checkbox', { name: /Streamo/ }))
    expect(resultat(vue)).toContain('2 catégories réglées recevront leur montant simulé comme plafond mensuel.')
    await fireEvent.click(vue.getByRole('button', { name: 'Fixer comme plafond' }))
    await waitFor(() => expect(confirmation.en).not.toBeNull())
    expect(confirmation.en?.titre).toBe('Fixer 2 plafonds ?')
    expect(confirmation.en?.message?.replace(/\s/g, ' ')).toContain('Restaurants : 165,76 € par mois (au lieu de 150,00 €) et Shopping : 149,88 € par mois (aucun plafond avant).')
    confirmation.en?.resoudre({ ok: false, valeur: '' })
    await waitFor(() => expect(confirmation.en).toBeNull())
    expect(modifier).not.toHaveBeenCalled()
    await fireEvent.click(vue.getByRole('button', { name: 'Fixer comme plafond' }))
    await waitFor(() => expect(confirmation.en).not.toBeNull())
    confirmation.en?.resoudre({ ok: true, valeur: '' })
    await waitFor(() => expect(texte(vue.getByRole('status'))).toBe('2 plafonds fixés : Restaurants 165,76 €, Shopping 149,88 €.'))
    expect(modifier.mock.calls).toEqual([[3, { monthly_cap: 16_576 }], [4, { monthly_cap: 14_988 }]])
    modifier.mockRejectedValueOnce(new ApiError(503, 'unavailable', 'indisponible'))
    await fireEvent.click(vue.getByRole('button', { name: 'Fixer comme plafond' }))
    await waitFor(() => expect(confirmation.en).not.toBeNull())
    confirmation.en?.resoudre({ ok: true, valeur: '' })
    await waitFor(() => expect(texte(vue.getByRole('alert'))).toBe('Tirelire est indisponible pour le moment.'))
  })

  it('lien prérempli : cible d’une catégorie et abonnement coché, cible déjà atteinte signalée', async () => {
    const vue = render(TirelireDecision, { resume: null, simulation: simulationStats(), requete: 'cible=3:15000&cible=1:90000&arret=54-monthly' })
    await waitFor(() => expect(resultat(vue)).toMatch(/tu économises ?100,79 € ?par mois ?1 209,48 € ?par an/))
    expect(texte(vue.getByRole('group', { name: 'Restaurants' }))).toContain('−37 %')
    expect(vue.getByRole('checkbox', { name: /Streamo/ })).toBeChecked()
    expect(texte(vue.getByRole('alert'))).toBe('Logement : ta dépense habituelle (780,00 €) est déjà sous cette cible.')
    expect(texte(vue.getByRole('group', { name: 'Logement' }))).toMatch(/^Logement ?fixe ?0 %/)
    expect(service).toHaveBeenCalledWith('calendar', { categories: { 3: { target: 15_000 } }, arrets: ['54-monthly'] })
  })

  it('états honnêtes : base fragile, première période, rien à classer, opérations à classer, service en panne', () => {
    const fragile = simulationStats({ basis: { method: 'current', periods: 1, period_ids: ['2026-11'], from: '2026-11-11', to: '2026-11-26', fragile: true }, subscriptions: [] })
    const vue = render(TirelireDecision, { resume: resumePremier({ unclassified: { amount: 28_000, operations: 7, share_bp: 1265, distorting: true } }), simulation: fragile })
    expect(texte(vue.getByRole('region', { name: 'Simulateur' }))).toMatch(/base fragile ?Base fragile : aucun mois complet dans tes relevés, le calcul part de la période en cours \(du 11 au 26 nov\. 2026\)\./)
    expect(texte(vue.getByRole('group', { name: 'Arrêter un abonnement' }))).toContain('Aucun abonnement actif repéré dans tes relevés.')
    const bref = vue.getByRole('region', { name: 'Résumé de la période' })
    expect(texte(bref)).toContain('C’est ta première période : rien à comparer pour l’instant.')
    expect(within(bref).getByRole('link', { name: /faussent ce résumé : classe-les/ })).toHaveAttribute('href', '#/tirelire/a-classer')
    expect(within(bref).queryByRole('group')).toBeNull()
    expect(texte(vue.getByRole('region', { name: 'Leviers les plus rentables' }))).toContain('Aucun levier à proposer sur cette période')
    vue.unmount()
    tirelire.erreurSimulation = 'Tirelire est indisponible pour le moment.'
    tirelire.erreurResume = 'Tirelire est indisponible pour le moment.'
    const panne = render(TirelireDecision, { resume: null, simulation: null })
    expect(panne.getByRole('heading', { name: 'Simulateur indisponible' })).toBeInTheDocument()
    expect(panne.getAllByRole('alert')).toHaveLength(2)
  })
})

describe('extrait du résumé', () => {
  it('sur l’accueil avec la carte Simulateur, et dans la vue du mois', () => {
    const accueil = render(TirelireAccueil, { mois: moisStats(), resume: resumeStats() })
    const bref = accueil.getByRole('region', { name: 'Résumé de la période' })
    expect(texte(bref)).toMatch(/^Novembre en bref ?au 26 nov\. 2026 ?Au 26 nov\. 2026, tu as dépensé 2 213,40 € en novembre/)
    expect(texte(bref)).toContain('Premier levier : Restaurants, 86,80 € par mois.')
    expect(within(bref).getAllByRole('group')).toHaveLength(1)
    expect(within(bref).getByRole('link', { name: 'Résumé complet et leviers' })).toHaveAttribute('href', '#/tirelire/decision')
    expect(texte(accueil.getByRole('link', { name: /Simulateur/ }))).toMatch(/^Simulateur ?si je baisse une catégorie, combien j’économise par mois et par an$/)
    expect(accueil.getByRole('link', { name: /Simulateur/ })).toHaveAttribute('href', '#/tirelire/decision')
    accueil.unmount()
    expect(render(TirelireAccueil, { mois: moisStats() }).queryByRole('region', { name: 'Résumé de la période' })).toBeNull()
    const octobre = resumeStats({ next_id: '2026-11' })
    const mois = render(TirelireMois, { mois: moisStats(), resume: octobre })
    expect(within(mois.getByRole('region', { name: 'Résumé de la période' })).getByRole('link', { name: 'Résumé complet et leviers' })).toHaveAttribute('href', '#/tirelire/decision?periode=2026-11')
    mois.unmount()
    expect(render(TirelireMois, { mois: moisStats(), resume: resumeStats({ mode: 'statement' }) }).queryByRole('region', { name: 'Résumé de la période' })).toBeNull()
  })

  it('n’écrit rien quand il n’y a rien à dire', () => {
    const vue = render(TirelireBref, { resume: resumePremier({ history: 'ok', previous_reason: null, partial: false, previous: { period_id: '2026-10', periods: null, up: [], up_total: 0, down: [], down_total: 0 }, savings: { total: 0, rate_bp: null, previous_rate_bp: null } }) })
    const bref = vue.getByRole('region', { name: 'Résumé de la période' })
    expect(bref.querySelectorAll('p')).toHaveLength(2)
    expect(texte(bref)).toMatch(/tu as dépensé 2 213,40 € en novembre\. ?Rien de notable : aucune catégorie ne s’écarte d’au moins 10 € et 10 % de ses repères\.$/)
  })
})
