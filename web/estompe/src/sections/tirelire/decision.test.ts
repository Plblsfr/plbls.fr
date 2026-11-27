import { describe, expect, it } from 'vitest'
import { hrefs } from '../../lib/route.svelte'
// @ts-expect-error pas de @types/node dans le projet : le test tourne sous Node via vitest
import { readFileSync } from 'node:fs'
import { de, detailLevier, lienLevier, messagePlafonds, phraseAClasser, phraseBase, phraseBudget, phraseDepenses, phraseEffet, phraseEpargne, phraseEtat, phraseLevier, phraseVariations, referenceDe, titreLevier, titreResume, variations, type Comparaison, type Levier, type Lien, type Variation } from './decision'
import { baisse, cleReglages, corpsSimulation, economie, effet, plafondsDe, pointsDeBase, prefillDe, reglagesDe, reglagesVides, SANS_REGLAGE, simuler, type Reference, type Reglages } from './simulation'
import { resumePremier, resumeStats, simulationStats } from './essais-decision'
import { periodeStats } from './essais'

interface Cas {
  economie: { nom: string; base: number; arretes: number; percent: number | null; target: number | null; economie: number; montant: number }[]
  effet: { nom: string; reference: Reference; categories: number; abonnements_par_mois: number; abonnements_par_an: number; attendu: ReturnType<typeof effet> }[]
}
const lire = (f: string): string => readFileSync(new URL(f, import.meta.url), 'utf8')
const CAS = JSON.parse(lire('../../../../../api/tirelire/tests/fixtures/simulation.json')) as Cas
const t = (s: string) => s.replace(/\s/g, ' ')
const adresse = (l: Lien) => hrefs.tirelire(l.section, l.id, l.requete)
const MAQUETTE: Reglages = { categories: { 3: { percent: 30 }, 5: { percent: 25 }, 4: { percent: 20 }, 2: { percent: 5 } }, arrets: [] }
const variation = (name: string, delta: number, o: Partial<Variation> = {}): Variation => ({ category_id: 1, name, kind: 'variable', color: 0, amount: 0, reference_amount: 0, delta, delta_bp: null, ...o })
const avril = () => resumeStats({ period: periodeStats({ id: '2026-04', from: '2026-04-01', to: '2026-04-30', known_through: '2026-04-30', elapsed_days: 30, complete: true, open: false }), partial: false, previous_id: '2026-03', previous: { period_id: '2026-03', periods: null, up: [], up_total: 0, down: [], down_total: 0 } })
const levier = (o: Partial<Levier>): Levier => ({ rank: 1, kind: 'over_cap', category_id: 3, merchant_id: null, recurring_id: null, name: 'Restaurants', amount: 23_680, reference_amount: 15_000, periods: null, count: null, monthly: 8680, yearly: 104_160, ...o })

describe('formule du simulateur, mêmes cas que le service', () => {
  it('économie d’une catégorie : pourcentage, cible, abonnement arrêté, arrondis et bornes', () => {
    expect(CAS.economie).toHaveLength(19)
    for (const c of CAS.economie) expect(economie(c.base, c.arretes, c.percent, c.target), c.nom).toEqual({ economie: c.economie, montant: c.montant })
  })

  it('effet sur les dépenses, le reste à vivre, le taux d’épargne et le budget', () => {
    expect(CAS.effet).toHaveLength(3)
    for (const c of CAS.effet) expect(effet(c.reference, c.categories, c.abonnements_par_mois, c.abonnements_par_an), c.nom).toEqual(c.attendu)
  })

  it('points de base arrondis au plus proche, absents sans dénominateur', () => {
    expect([pointsDeBase(1, 3), pointsDeBase(-25_000, 25_000), pointsDeBase(100, 0), pointsDeBase(100, -5), pointsDeBase(47_037, 270_000)]).toEqual([3333, -10_000, null, null, 1742])
  })
})

describe('simuler', () => {
  it('plusieurs catégories : total par mois et par an, reste à vivre, taux d’épargne', () => {
    const s = simuler(simulationStats(), MAQUETTE)
    expect(s.categories.filter((c) => c.saving).map((c) => [c.name, c.saving, c.amount])).toEqual([['Courses', 2063, 39_197], ['Restaurants', 7104, 16_576], ['Shopping', 3747, 14_988], ['Loisirs et sorties', 4123, 12_367]])
    expect(s.result).toEqual({ monthly: 17_037, yearly: 204_444, categories: 17_037, subscriptions: 0, expenses: 187_363, remaining: 52_637, savings: 47_037, savings_rate_bp: 1742, budget_delta: -17_637 })
    expect(simuler(simulationStats(), SANS_REGLAGE).result).toEqual(simulationStats().result)
  })

  it('abonnements arrêtés : retirés de leur catégorie avant la baisse, coût annuel exact', () => {
    const s = simuler(simulationStats(), { categories: { 9: { percent: 10 }, 2: { target: 30_000 } }, arrets: ['54-monthly', '56-weekly'] })
    const abonnements = s.categories.find((c) => c.id === 9)!
    expect([abonnements.stopped, abonnements.saving, abonnements.amount]).toEqual([1399, 1045, 9403])
    const courses = s.categories.find((c) => c.id === 2)!
    expect([courses.stopped, courses.saving, courses.amount]).toEqual([6500, 4760, 30_000])
    expect(s.subscriptions.map((a) => a.stopped)).toEqual([true, false, true])
    expect([s.result.categories, s.result.subscriptions, s.result.monthly, s.result.yearly]).toEqual([5805, 7899, 13_704, 5805 * 12 + 94_788])
    expect(baisse(abonnements)).toBe(10)
    expect(baisse(courses)).toBe(14)
    expect(baisse({ base: 0, stopped: 0, saving: 0 })).toBe(0)
  })

  it('catégorie sans historique et bornes 0 et 100 %', () => {
    const s = simuler(simulationStats(), { categories: { 7: { percent: 50 }, 6: { percent: 100 }, 4: { percent: 0 } }, arrets: [] })
    expect(s.categories.filter((c) => [7, 6, 4].includes(c.id)).map((c) => [c.id, c.saving, c.amount])).toEqual([[4, 0, 18_735], [6, 14_800, 0], [7, 0, 0]])
  })

  it('corps de la demande, clé, lien prérempli et plafonds proposés', () => {
    expect(corpsSimulation('statement', { categories: { 5: { target: 12_000 }, 3: { percent: 30 } }, arrets: ['56-weekly', '54-monthly'] })).toEqual({ mode: 'statement', categories: [{ id: 3, percent: 30 }, { id: 5, target: 12_000 }], subscriptions: ['54-monthly', '56-weekly'] })
    expect(cleReglages('calendar', { categories: { 3: { percent: 30 } }, arrets: [] })).toBe(cleReglages('calendar', { arrets: [], categories: { 3: { percent: 30 } } }))
    expect([reglagesVides(SANS_REGLAGE), reglagesVides(MAQUETTE)]).toEqual([true, false])
    expect(prefillDe('cible=3:15000&cible=2:38000&cible=x:1&cible=4&arret=54-monthly&arret=%3Cb%3E&periode=2026-10')).toEqual({ cibles: [{ id: 3, montant: 15_000 }, { id: 2, montant: 38_000 }], arrets: ['54-monthly'] })
    expect(prefillDe()).toEqual({ cibles: [], arrets: [] })
    const { reglages, dejaSous } = reglagesDe(simulationStats(), { cibles: [{ id: 3, montant: 15_000 }, { id: 6, montant: 20_000 }, { id: 99, montant: 1 }], arrets: ['54-monthly', '1-monthly'] })
    expect(reglages).toEqual({ categories: { 3: { target: 15_000 } }, arrets: ['54-monthly'] })
    expect(dejaSous.map((c) => c.name)).toEqual(['Transports'])
    const regles: Reglages = { categories: { 3: { percent: 30 }, 4: { percent: 20 }, 6: { percent: 0 } }, arrets: ['54-monthly'] }
    const plafonds = plafondsDe(simuler(simulationStats(), regles), regles)
    expect(plafonds).toEqual([{ id: 3, name: 'Restaurants', cap: 16_576, avant: 15_000 }, { id: 4, name: 'Shopping', cap: 14_988, avant: null }])
    expect(t(messagePlafonds(plafonds).message)).toBe('Restaurants : 165,76 € par mois (au lieu de 150,00 €) et Shopping : 149,88 € par mois (aucun plafond avant). Ils remplacent les plafonds actuels de ces catégories ; tout reste modifiable dans Catégories et règles.')
    expect(messagePlafonds(plafonds).titre).toBe('Fixer 2 plafonds ?')
    expect(messagePlafonds(plafonds.slice(0, 1)).titre).toBe('Fixer ce plafond ?')
    expect(t(messagePlafonds(plafonds.slice(0, 1)).message)).toContain('Il remplace le plafond actuel de cette catégorie')
  })
})

describe('phrases du simulateur', () => {
  it('base : nombre de périodes, singulier, relevés, base fragile', () => {
    const s = simulationStats()
    expect(t(phraseBase(s))).toBe('Base : ta dépense moyenne de chaque catégorie sur tes 3 derniers mois complets (du 1er août au 31 oct. 2026).')
    expect(t(phraseBase({ mode: 'calendar', basis: { ...s.basis, periods: 1, from: '2026-10-01' } }))).toBe('Base : ta dépense moyenne de chaque catégorie sur ton dernier mois complet (du 1er au 31 oct. 2026).')
    expect(t(phraseBase({ mode: 'statement', basis: { ...s.basis, periods: 2 } }))).toContain('sur tes 2 derniers relevés (')
    expect(t(phraseBase({ mode: 'statement', basis: { ...s.basis, periods: 1 } }))).toContain('sur ton dernier relevé (')
    expect(t(phraseBase({ mode: 'calendar', basis: { method: 'current', periods: 1, period_ids: ['2026-01'], from: '2026-01-11', to: '2026-01-31', fragile: true } }))).toBe('Base fragile : aucun mois complet dans tes relevés, le calcul part de la période en cours (du 11 au 31 janv. 2026).')
  })

  it('effet : taux d’épargne et reste à vivre, sans revenu, sans réglage, budget', () => {
    const s = simuler(simulationStats(), MAQUETTE)
    expect(t(phraseEffet(s))).toBe('Si tu épargnes cette somme, ton taux d’épargne passe de 11,1 % à 17,4 %. Sinon, ton reste à vivre passe de 356,00 € à 526,37 €.')
    expect(t(phraseBudget(s))).toBe('Le trait marque ton budget de 2 050,00 € : tu serais 176,37 € en dessous.')
    expect(phraseEffet(simulationStats())).toBe('Déplace un curseur, saisis une cible ou coche un abonnement pour voir ce que tu économiserais.')
    const sansRevenu = simuler(simulationStats({ reference: { expenses: 204_400, income: 0, savings: 0, remaining: -204_400, savings_rate_bp: null, budget: null } }), MAQUETTE)
    expect(t(phraseEffet(sansRevenu))).toBe('Sans revenu sur la période de base, pas de taux d’épargne ; ton reste à vivre passe de −2 044,00 € à −1 873,63 €.')
    expect(phraseBudget(sansRevenu)).toBe('')
    const serre = (budget: number) => t(phraseBudget(simuler(simulationStats({ reference: { expenses: 204_400, income: 270_000, savings: 30_000, remaining: 35_600, savings_rate_bp: 1111, budget } }), MAQUETTE)))
    expect(serre(190_000)).toBe('Le trait marque ton budget de 1 900,00 € : tu repasserais 26,37 € en dessous.')
    expect(serre(180_000)).toBe('Le trait marque ton budget de 1 800,00 € : tu resterais 73,63 € au-dessus.')
  })
})

describe('phrases du résumé', () => {
  it('dépenses : période en cours, mois complet, par relevé, sans comparaison', () => {
    expect(titreResume(resumeStats())).toBe('Novembre en bref')
    expect(titreResume(resumeStats({ mode: 'statement' }))).toBe('Ce relevé en bref')
    expect(t(phraseDepenses(resumeStats()))).toBe('Au 26 nov. 2026, tu as dépensé 2 213,40 € en novembre, soit 186,40 € de plus qu’en octobre (+9 %) et 163,40 € au-dessus de ton budget.')
    const a = avril()
    expect(t(phraseDepenses({ ...a, expenses: { total: 180_000, previous: { amount: 200_000, delta: -20_000, delta_bp: -1000 }, usual: null, budget: 185_000, budget_delta: -5000 } }))).toBe('Tu as dépensé 1 800,00 € en avril, soit 200,00 € de moins qu’en mars (−10 %) et 50,00 € sous ton budget.')
    expect(t(phraseDepenses({ ...a, expenses: { total: 180_000, previous: { amount: 180_000, delta: 0, delta_bp: 0 }, usual: null, budget: null, budget_delta: null } }))).toBe('Tu as dépensé 1 800,00 € en avril, soit autant qu’en mars.')
    expect(t(phraseDepenses(resumeStats({ mode: 'statement', period: periodeStats({ id: '2026-11-26', from: '2026-10-31', to: '2026-11-26', complete: true, open: false }), partial: false })))).toBe('Tu as dépensé 2 213,40 € sur le relevé du 26 nov. 2026, soit 186,40 € de plus que sur le relevé précédent (+9 %) et 163,40 € au-dessus de ton budget.')
    expect(t(phraseDepenses(resumePremier()))).toBe('Au 26 nov. 2026, tu as dépensé 2 213,40 € en novembre.')
    expect(t(phraseDepenses(resumeStats({ expenses: { total: 150_000, previous: { amount: 202_700, delta: -52_700, delta_bp: -2600 }, usual: null, budget: 205_000, budget_delta: -55_000 } })))).toBe('Au 26 nov. 2026, tu as dépensé 1 500,00 € en novembre, soit encore 550,00 € sous ton budget.')
  })

  it('hausses et baisses : singulier, pluriel, liste tronquée, référence', () => {
    const r = resumeStats()
    expect(t(phraseVariations(r, r.previous!, 'up'))).toBe('Par rapport à octobre, 2 catégories sont en hausse : Restaurants (+88,80 €) et Courses (+41,60 €).')
    expect(t(phraseVariations(r, r.usual!, 'up'))).toBe('Par rapport à ta moyenne des 3 derniers mois complets, 1 catégorie est en hausse : Restaurants (+63,80 €).')
    expect(phraseVariations(r, r.previous!, 'down')).toBe('')
    expect(variations(r.previous!, 'down')).toBe('')
    const baisses = (total: number): Comparaison => ({ period_id: '2026-03', periods: null, up: [], up_total: 0, down: [variation('Shopping', -2505), variation('Loisirs et sorties', -1840), variation('Énergie', -1200)], down_total: total })
    const a = avril()
    expect(t(phraseVariations(a, baisses(3), 'down'))).toBe('Par rapport à mars, 3 catégories sont en baisse : Shopping (−25,05 €), Loisirs et sorties (−18,40 €) et Énergie (−12,00 €).')
    expect(t(variations(baisses(4), 'down'))).toBe('Shopping (−25,05 €), Loisirs et sorties (−18,40 €), Énergie (−12,00 €) et 1 autre catégorie')
    expect(t(variations(baisses(6), 'down'))).toContain('et 3 autres catégories')
    const releve = resumeStats({ mode: 'statement' })
    expect(referenceDe(releve, releve.previous!)).toBe('Par rapport au relevé précédent')
    expect(referenceDe(releve, releve.usual!)).toBe('Par rapport à ta moyenne des 3 derniers relevés')
    expect(referenceDe(releve, { ...releve.usual!, periods: 1 })).toBe('Par rapport au relevé d’avant')
    expect(referenceDe(r, { ...r.usual!, periods: 1 })).toBe('Par rapport à ton dernier mois complet')
  })

  it('épargne, opérations à classer et états honnêtes', () => {
    expect(t(phraseEpargne(resumeStats()))).toBe('Taux d’épargne de 8,8 %, contre 11,1 % en octobre.')
    expect(t(phraseEpargne(resumePremier()))).toBe('Taux d’épargne de 8,8 %.')
    expect(t(phraseEpargne(resumeStats({ mode: 'statement' })))).toBe('Taux d’épargne de 8,8 %, contre 11,1 % sur le relevé précédent.')
    expect(phraseEpargne(resumeStats({ savings: { total: 0, rate_bp: null, previous_rate_bp: 1109 } }))).toBe('')
    expect(phraseAClasser(resumeStats())).toBe('')
    expect(t(phraseAClasser(resumeStats({ unclassified: { amount: 28_000, operations: 7, share_bp: 1265, distorting: true } })))).toBe('7 opérations à classer (280,00 €, 13 % des dépenses) faussent ce résumé : classe-les pour le fiabiliser.')
    expect(t(phraseAClasser(resumeStats({ unclassified: { amount: 28_000, operations: 1, share_bp: 1265, distorting: true } })))).toBe('1 opération à classer (280,00 €, 13 % des dépenses) fausse ce résumé : classe-la pour le fiabiliser.')
    expect(phraseEtat(resumePremier())).toBe('C’est ta première période : rien à comparer pour l’instant. Les écarts apparaîtront avec un deuxième mois.')
    expect(phraseEtat(resumePremier({ mode: 'statement' }))).toContain('avec un deuxième relevé.')
    expect(phraseEtat(resumePremier({ history: 'insufficient', previous_reason: 'incomplete' }))).toBe('Données insuffisantes pour comparer : le mois précédent n’est couvert qu’en partie par tes relevés, et aucun mois complet ne permet encore de calculer une moyenne.')
    expect(phraseEtat(resumePremier({ history: 'insufficient' }))).toContain('aucun relevé ne couvre le mois précédent')
    expect(phraseEtat(resumeStats())).toBe('La période est en cours : les baisses ne sont pas citées tant qu’elle n’est pas terminée.')
    expect(phraseEtat({ ...avril(), usual: null })).toBe('Rien de notable : aucune catégorie ne s’écarte d’au moins 10 € et 10 % de ses repères.')
    expect(phraseEtat({ ...avril(), previous: { period_id: '2026-03', periods: null, up: [variation('Courses', 1500)], up_total: 1, down: [], down_total: 0 } })).toBe('')
    expect(phraseEtat({ ...avril(), previous: null, previous_reason: 'incomplete' })).toBe('Pas de comparaison avec le mois précédent : tes relevés ne le couvrent qu’en partie.')
  })

  it('leviers : détail, élisions, gain, lien', () => {
    const r = resumeStats()
    expect(r.levers.map(titreLevier)).toEqual(['Restaurants', 'Petits achats répétés', 'Courses'])
    expect(t(phraseLevier(r.levers[0], r))).toBe('Restaurants : revenir au plafond de 150,00 € (dépensé : 236,80 €). Gain estimé : 86,80 € par mois, 1 041,60 € par an.')
    expect(t(detailLevier(r.levers[1], r))).toBe('en éviter un sur deux : 96 petits achats sur 3 mois, surtout chez Épicerie du Canal')
    expect(t(detailLevier(levier({ kind: 'small_purchases', name: null, count: 1, periods: 1 }), resumeStats({ mode: 'statement' })))).toBe('en éviter un sur deux : 1 petit achat sur 1 relevé')
    expect(t(detailLevier(levier({ kind: 'over_average', amount: 30_000, reference_amount: 12_000, periods: 3 }), r))).toBe('revenir à ta moyenne de 120,00 € sur 3 mois (dépensé : 300,00 €)')
    expect(t(detailLevier(levier({ kind: 'over_average', amount: 30_000, reference_amount: 12_000, periods: 2 }), resumeStats({ mode: 'statement' })))).toContain('sur 2 relevés')
    const hausse = levier({ kind: 'merchant_up', category_id: 5, merchant_id: 6, name: 'Ciné Phare', amount: 3900, reference_amount: 1000 })
    expect(t(detailLevier(hausse, r))).toBe('revenir au niveau d’octobre : 10,00 € au lieu de 39,00 €')
    expect(t(detailLevier(hausse, avril()))).toBe('revenir au niveau de mars : 10,00 € au lieu de 39,00 €')
    expect(t(detailLevier(hausse, resumeStats({ mode: 'statement' })))).toBe('revenir au niveau du relevé précédent : 10,00 € au lieu de 39,00 €')
    expect(t(detailLevier(levier({ kind: 'subscription_changed', name: 'Streamo', amount: 1399, reference_amount: 1199 }), r))).toBe('revenir à l’ancien tarif de 11,99 € (passé à 13,99 €)')
    expect(t(detailLevier(levier({ kind: 'subscription_new', name: 'Arcadia+', amount: 699, reference_amount: 0 }), r))).toBe('nouvel abonnement (6,99 € à chaque échéance) : en as-tu besoin ?')
    expect(titreLevier(levier({ kind: 'merchant_up', name: null }))).toBe('Commerçant sans nom')
    expect([de('avril'), de('mars'), de('août'), de('octobre'), de('Épicerie du Canal'), de('Livréo')]).toEqual(['d’avril', 'de mars', 'd’août', 'd’octobre', 'd’Épicerie du Canal', 'de Livréo'])
    expect(adresse(lienLevier(r.levers[0], r))).toBe('#/tirelire/decision?cible=3:15000')
    expect(adresse(lienLevier(r.levers[1], r))).toBe('#/tirelire/habitudes')
    expect(adresse(lienLevier(hausse, r))).toBe('#/tirelire/operations?from=2026-11-01&to=2026-11-30&merchant=6&direction=debit&date=purchase')
    expect(adresse(lienLevier(levier({ kind: 'subscription_new', recurring_id: '55-monthly' }), r))).toBe('#/tirelire/decision?arret=55-monthly')
    expect(adresse(lienLevier(levier({ kind: 'subscription_changed', recurring_id: '54-monthly' }), r))).toBe('#/tirelire/habitudes/abonnements')
  })
})
