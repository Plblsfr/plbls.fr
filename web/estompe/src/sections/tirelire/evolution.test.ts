import { describe, expect, it } from 'vitest'
import { annees, arrondi, colonne, compte, debutsDeMois, detailTendance, ecartJours, echelleSolde, etendue, etiquetteTendance, lireFenetre, nomFenetre, paliers, plusJours, soldeLe, suites, tranches } from './evolution'
import { historiqueStats, soldesStats, tendanceStats } from './essais'
import { teinte } from './tirelire'

const serre = (t: string) => t.replace(/[\s  ]+/g, ' ')

describe('fenêtre', () => {
  it('lit la fenêtre mémorisée et la nomme selon le mode', () => {
    expect(['6', '12', '24', 'all', '7', null, undefined].map(lireFenetre)).toEqual(['6', '12', '24', 'all', '12', '12', '12'])
    expect(nomFenetre('6', 'calendar')).toBe('6 mois')
    expect(nomFenetre('24', 'statement')).toBe('24 relevés')
    expect(nomFenetre('all', 'statement')).toBe('Tout')
  })
})

describe('colonnes et étendue', () => {
  const h = historiqueStats()
  it('mois courts, année au premier mois et à chaque changement d’année', () => {
    expect(h.periods.map((p) => colonne(p, 'calendar'))).toEqual(['juin', 'juil.', 'août', 'sept.', 'oct.', 'nov.'])
    expect(annees(h.periods, 'calendar')).toEqual(['2026', '', '', '', '', ''])
    expect(annees([{ from: '2025-12-01', to: '2025-12-31' }, { from: '2026-01-01', to: '2026-01-31' }, { from: '2026-02-01', to: '2026-02-28' }], 'calendar')).toEqual(['2025', '2026', ''])
    expect(colonne({ id: '2026-11-26', from: '2026-10-31', to: '2026-11-26' }, 'statement')).toBe('26/11')
    expect(etendue([{ id: '2025-12-26', from: '2025-11-30', to: '2025-12-26' }, { id: '2026-11-26', from: '2026-10-31', to: '2026-11-26' }], 'statement')).toBe('26 déc. 2025 → 26 nov. 2026')
    expect(annees([{ from: '2025-12-31', to: '2026-01-31' }], 'statement')).toEqual(['2026'])
  })
  it('étendue de la fenêtre', () => {
    expect(etendue(h.periods, 'calendar')).toBe('juin 2026 → nov. 2026')
    expect(etendue(h.periods.slice(0, 1), 'calendar')).toBe('juin 2026')
    expect(etendue([], 'calendar')).toBe('')
  })
  it('montants arrondis à l’euro, sans symbole', () => {
    expect([78_000, 23_680, 49, 50, 0, -1250, 221_340].map(arrondi).map(serre)).toEqual(['780', '237', '0', '1', '0', '−13', '2 213'])
  })
})

describe('tendance', () => {
  it('étiquette : sens et variation, jamais la couleur seule', () => {
    expect(serre(etiquetteTendance(tendanceStats()))).toBe('hausse +22 %')
    expect(serre(etiquetteTendance(tendanceStats({ direction: 'down', delta: -1600, delta_bp: -1077 })))).toBe('baisse −11 %')
    expect(serre(etiquetteTendance(tendanceStats({ direction: 'stable', delta: 0, delta_bp: 0 })))).toBe('stable 0 %')
    expect(serre(etiquetteTendance(tendanceStats({ direction: 'stable', delta: 0, delta_bp: null, average: 0 })))).toBe('stable')
    expect(etiquetteTendance(tendanceStats({ direction: 'insufficient', periods: 2, average: null, delta: null, delta_bp: null }))).toBe('pas assez de données')
  })
  it('détail chiffré et nombre de périodes', () => {
    expect(serre(detailTendance(tendanceStats(), 'calendar'))).toBe('+30 € en 5 mois · moy. 139 €')
    expect(serre(detailTendance(tendanceStats({ direction: 'down', delta: -1600 }), 'statement'))).toBe('−16 € en 5 relevés · moy. 139 €')
    expect(detailTendance(tendanceStats({ direction: 'insufficient', periods: 1, average: null, delta: null, delta_bp: null }), 'calendar')).toBe('1 mois complet sur 3 nécessaires')
    expect(detailTendance(tendanceStats({ direction: 'insufficient', periods: 2, average: null, delta: null, delta_bp: null }), 'statement')).toBe('2 relevés complets sur 3 nécessaires')
  })
  it('compte les sens', () => {
    expect(compte(historiqueStats().categories)).toEqual({ up: 1, down: 1, stable: 3, insufficient: 0 })
  })
})

describe('tranches du graphique', () => {
  it('catégories retenues, Autres puis À classer ; leur somme est le total de la période', () => {
    const h = historiqueStats()
    for (let k = 0; k < h.periods.length; k++) {
      const t = tranches(h, k, teinte)
      expect(t.map((x) => x.nom)).toEqual(['Logement', 'Courses', 'Restaurants', 'Transports', 'Autres (1)', 'À classer'])
      expect(t.map((x) => x.teinte)).toEqual([1, 2, 3, 6, 'autres', 'classer'])
      expect(t.reduce((n, x) => n + x.montant, 0)).toBe(h.total.amounts[k])
    }
  })
  it('ni Autres ni À classer quand ils sont vides', () => {
    const h = historiqueStats()
    const sans = { ...h, chart: { ...h.chart, others_categories: 0 }, unclassified: { amounts: [0, 0, 0, 0, 0, 0], total: 0, average: 0 } }
    expect(tranches(sans, 5, teinte).map((x) => x.nom)).toEqual(['Logement', 'Courses', 'Restaurants', 'Transports'])
  })
})

describe('solde', () => {
  const c = soldesStats().accounts[0]
  it('dates et jours', () => {
    expect(ecartJours('2026-10-01', '2026-11-26')).toBe(56)
    expect(plusJours('2026-12-30', 3)).toBe('2027-01-02')
    expect(ecartJours('2026-03-28', '2026-03-30')).toBe(2)
  })
  it('solde d’un jour, absent dans un trou', () => {
    expect(soldeLe(c, '2026-10-01')).toBe(120_000)
    expect(soldeLe(c, '2026-10-05')).toBe(-4500)
    expect(soldeLe(c, '2026-10-26')).toBe(152_340)
    expect(soldeLe(c, '2026-10-28')).toBeNull()
    expect(soldeLe(c, '2026-11-26')).toBe(190_000)
  })
  it('paliers : un jour par changement, les trous à leur place', () => {
    expect(paliers(c)).toEqual([
      { date: '2026-10-01', amount: 120_000 }, { date: '2026-10-05', amount: -4500 }, { date: '2026-10-21', amount: 152_340 },
      { date: '2026-10-27', amount: null, fin: '2026-10-31' },
      { date: '2026-11-01', amount: 152_340 }, { date: '2026-11-11', amount: 61_000 }, { date: '2026-11-23', amount: 190_000 },
    ])
  })
  it('échelle qui contient zéro, le plus bas et le plus haut', () => {
    expect(echelleSolde(-4500, 190_000)).toEqual({ min: -100_000, max: 200_000, valeurs: [-100_000, 0, 100_000, 200_000] })
    expect(echelleSolde(50_000, 720_000)).toEqual({ min: 0, max: 750_000, valeurs: [0, 250_000, 500_000, 750_000] })
    const plat = echelleSolde(0, 0)
    expect(plat.min).toBe(0)
    expect(plat.max).toBeGreaterThan(0)
  })
  it('graduations au premier de chaque mois, espacées quand il y en a trop', () => {
    expect(debutsDeMois('2026-10-01', '2026-11-26', 6)).toEqual(['2026-10-01', '2026-11-01'])
    expect(debutsDeMois('2025-11-15', '2026-02-10', 6)).toEqual(['2025-12-01', '2026-01-01', '2026-02-01'])
    expect(debutsDeMois('2026-01-01', '2026-12-31', 4)).toEqual(['2026-03-01', '2026-06-01', '2026-09-01', '2026-12-01'])
  })
})

describe('suites', () => {
  it('coupe aux valeurs absentes, sans interpoler', () => {
    expect(suites([null, 1, 2, null, null, 5, null])).toEqual([{ debut: 1, valeurs: [1, 2] }, { debut: 5, valeurs: [5] }])
    expect(suites([null, null])).toEqual([])
  })
})
