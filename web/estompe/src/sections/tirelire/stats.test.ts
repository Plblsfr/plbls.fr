import { describe, expect, it } from 'vitest'
import { moisStats, periodeStats } from './essais'
import { abscisse, arcs, ecartCourt, etatPeriode, etincelle, eurosRonds, eurosRondsSignes, filtresPeriode, graduations, jauge, joursGradues, lireMode, nomPrecedente, ordonnee, parRang, phraseProjection, pourcent, pourcentSigne, referenceDe, sensEcart, titrePeriode, trace, type Depassement } from './stats'
import { filtres, filtresDe, pastilles, requeteDe } from './tirelire'

const propre = (t: string) => t.replace(/[\u00a0\u202f]/g, ' ')

describe('formats des statistiques', () => {
  it('arrondit les euros et les pourcentages, signe compris', () => {
    expect(propre(eurosRonds(243_049))).toBe('2 430 €')
    expect(propre(eurosRonds(-150))).toBe('−2 €')
    expect(propre(eurosRondsSignes(38_000))).toBe('+380 €')
    expect(propre(pourcent(10_797))).toBe('108 %')
    expect(propre(pourcent(880, 1))).toBe('8,8 %')
    expect(propre(pourcent(-1250))).toBe('−13 %')
    expect(propre(pourcentSigne(5787))).toBe('+58 %')
    expect(propre(pourcentSigne(-576))).toBe('−6 %')
    expect(propre(pourcentSigne(30))).toBe('= 0 %')
  })

  it('écart : pourcentage si la référence le permet, sinon euros ; stable sous 0,5 %', () => {
    expect(propre(ecartCourt({ amount: 1000, delta: 250, delta_bp: 2500 }))).toBe('+25 %')
    expect(propre(ecartCourt({ amount: 0, delta: 2800, delta_bp: null }))).toBe('+28,00 €')
    expect(propre(ecartCourt({ amount: 0, delta: 0, delta_bp: null }))).toBe('= 0 %')
    expect(sensEcart({ amount: 78_000, delta: 300, delta_bp: 38 })).toBe('egal')
    expect(sensEcart({ amount: 1000, delta: -250, delta_bp: -2500 })).toBe('baisse')
    expect(sensEcart({ amount: 0, delta: 10, delta_bp: null })).toBe('hausse')
  })

  it('nomme la période selon le mode', () => {
    expect(titrePeriode(periodeStats(), 'calendar')).toBe('Novembre 2026')
    expect(titrePeriode(periodeStats({ from: '2026-10-07', to: '2026-11-07' }), 'statement')).toBe('Relevé du 7 nov. 2026')
    expect(nomPrecedente('2026-10', 'calendar')).toBe('octobre')
    expect(nomPrecedente('2026-10-07', 'statement')).toBe('relevé précédent')
    expect(etatPeriode(periodeStats(), 'calendar')).toBe('Jour 26 sur 30 · données jusqu’au 26 nov. 2026')
    expect(etatPeriode(periodeStats({ open: false, complete: true }), 'calendar')).toBe('Du 1er au 30 nov. 2026 · mois complet')
    expect(etatPeriode(periodeStats({ open: false, complete: false }), 'calendar')).toContain('mois incomplet')
    expect(etatPeriode(periodeStats({ from: '2026-10-07', to: '2026-11-07', days: 31 }), 'statement')).toBe('Du 7 oct. au 7 nov. 2026 · 31 jours')
    expect(lireMode('statement')).toBe('statement')
    expect(lireMode('autre')).toBe('calendar')
    expect(lireMode(null)).toBe('calendar')
  })

  it('phrase de projection : rythme, jours restants et charges fixes attendues', () => {
    expect(propre(phraseProjection(moisStats()))).toBe('Au rythme des 26 premiers jours : environ 54 € par jour de dépenses variables pendant 4 jours, plus 62 € de charges fixes habituelles pas encore passées.')
    expect(phraseProjection(moisStats({ projection: null }))).toBe('')
  })
})

describe('liens vers les opérations', () => {
  it('mois calendaire : période par date d’achat ; relevé : filtre par relevé', () => {
    const calendaire = filtresPeriode(periodeStats(), 'calendar', { categorie: 3 })
    expect(requeteDe(calendaire)).toBe('from=2026-11-01&to=2026-11-30&category=3&date=purchase')
    expect(requeteDe(filtresPeriode(periodeStats({ statement_ids: [11] }), 'statement', { categorie: 3 }))).toBe('statement=11&category=3')
    expect(requeteDe(filtresPeriode(periodeStats({ from: '2026-10-07', to: '2026-11-07', statement_ids: [11, 12] }), 'statement'))).toBe('from=2026-10-07&to=2026-11-07')
    expect(filtresDe('from=2026-11-01&to=2026-11-30&category=3&date=purchase')).toEqual(calendaire)
    expect(filtresDe('date=purchase').parAchat).toBe(false)
    expect(requeteDe(filtres({ parAchat: true }))).toBe('')
    const p = pastilles(calendaire, { releves: [], categories: [], commercants: [] }).find((x) => x.cle === 'achat')
    expect([p?.texte, p?.sans]).toEqual(['Par date d’achat', { parAchat: false }])
  })
})

describe('classement des dépassements', () => {
  const d = (category_id: number, rank_amount: number, rank_percent: number): Depassement => ({ category_id, reference: 'cap', reference_amount: 10_000, periods: null, excess: 100, excess_bp: 100, rank_amount, rank_percent })

  it('suit le rang calculé par le service, en euros ou en pourcentage', () => {
    const liste = [d(1, 1, 3), d(2, 2, 1), d(3, 3, 2)]
    expect(parRang(liste, 'euros').map((x) => x.category_id)).toEqual([1, 2, 3])
    expect(parRang(liste, 'pourcent').map((x) => x.category_id)).toEqual([2, 3, 1])
    expect(liste.map((x) => x.category_id)).toEqual([1, 2, 3])
  })

  it('nomme la référence utilisée', () => {
    expect(propre(referenceDe(d(1, 1, 1), 'calendar'))).toBe('plafond 100,00 €')
    expect(propre(referenceDe({ ...d(1, 1, 1), reference: 'average', reference_amount: 3300, periods: 2 }, 'calendar'))).toBe('moyenne 33,00 € sur 2 mois')
    expect(propre(referenceDe({ ...d(1, 1, 1), reference: 'average', reference_amount: 3300, periods: 2 }, 'statement'))).toBe('moyenne 33,00 € sur 2 relevés')
  })
})

describe('géométrie des graphiques', () => {
  it('graduations rondes au-dessus du maximum', () => {
    expect(graduations(243_000)).toEqual({ max: 300_000, pas: 100_000, valeurs: [100_000, 200_000, 300_000] })
    expect(graduations(23_680, 2)).toEqual({ max: 40_000, pas: 20_000, valeurs: [20_000, 40_000] })
    expect(graduations(0).max).toBeGreaterThan(0)
  })

  it('échelles et tracé', () => {
    const cadre = { largeur: 300, hauteur: 200, gauche: 0, droite: 0, haut: 0, bas: 0 }
    const x = abscisse(cadre, 31)
    const y = ordonnee(cadre, 1000)
    expect([x(1), x(16), x(31)]).toEqual([0, 150, 300])
    expect([y(0), y(500), y(1000), y(-50)]).toEqual([200, 100, 0, 200])
    expect(trace([0, 500, 1000], abscisse(cadre, 3), y)).toBe('M0.0 200.0L150.0 100.0L300.0 0.0')
    expect(abscisse(cadre, 1)(1)).toBe(0)
  })

  it('jours gradués : le dernier jour connu remplace ses voisins', () => {
    expect(joursGradues(30)).toEqual([1, 5, 10, 15, 20, 25, 30])
    expect(joursGradues(30, 26)).toEqual([1, 5, 10, 15, 20, 26, 30])
    expect(joursGradues(10)).toEqual([1, 5, 10])
  })

  it('courbe miniature : rien sous deux points, plate si tout est égal', () => {
    expect(etincelle([5], 60, 24)).toBeNull()
    expect(etincelle([5, 5, 5], 60, 24)?.chemin).toBe('M3.0 12.0L30.0 12.0L57.0 12.0')
    expect(etincelle([0, 10], 60, 24)).toEqual({ chemin: 'M3.0 21.0L57.0 3.0', x: 57, y: 3 })
  })

  it('jauge : partie sous la référence, excès et repère', () => {
    const j = jauge(15_000, 10_000)
    expect(j.plein + j.exces).toBeCloseTo(100 / 1.04)
    expect(j.exces).toBeCloseTo(j.plein / 2)
    expect(j.repere).toBeCloseTo(j.plein)
    expect(jauge(5000, 10_000).exces).toBe(0)
    expect(jauge(5000, null)).toEqual({ plein: 100, exces: 0, repere: null })
    expect(jauge(-200, 10_000).plein).toBe(0)
  })

  it('anneau : un tour pour 100 %, l’excès sur un second tour', () => {
    const tour = 2 * Math.PI * 50
    expect(arcs(5000).plein).toBeCloseTo(tour / 2)
    expect(arcs(10_797).plein).toBeCloseTo(tour)
    expect(arcs(10_797).exces).toBeCloseTo(tour * 0.0797)
    expect(arcs(-50)).toEqual({ plein: 0, exces: 0 })
  })
})
