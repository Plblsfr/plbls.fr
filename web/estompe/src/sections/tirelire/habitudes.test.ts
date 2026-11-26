import { describe, expect, it } from 'vitest'
import { habitudesStats, recurrent, recurrentsCourts, recurrentsStats } from './essais-habitudes'
import { actifs, alertes, arretes, cadenceDe, classes, couverture, ecartes, fois, forts, lienOperations, lireFenetreHabitudes, manque, nomFenetreHabitudes, nomSemaine, phraseAlerte, phraseJours, phraseSemaines, raisonDe, methodeRaison } from './habitudes'

const plat = (s: string) => s.replace(/[  ]/g, ' ')

describe('habitudes', () => {
  it('lit la fenêtre mémorisée et la nomme selon le découpage', () => {
    expect(['1', '3', '6', '12', 'all', '24', '', null, undefined].map(lireFenetreHabitudes)).toEqual(['1', '3', '6', '12', 'all', '3', '3', '3', '3'])
    expect([nomFenetreHabitudes('1', 'calendar'), nomFenetreHabitudes('1', 'statement'), nomFenetreHabitudes('6', 'calendar'), nomFenetreHabitudes('3', 'statement'), nomFenetreHabitudes('all', 'calendar')]).toEqual(['Ce mois', 'Ce relevé', '6 mois', '3 relevés', 'Tout'])
  })

  it('classe les commerçants par les rangs du service', () => {
    const h = habitudesStats()
    expect(classes(h, 'montant').map((m) => m.merchant_id)).toEqual([41, 42, 43, 44, 45])
    expect(classes(h, 'nombre').map((m) => m.merchant_id)).toEqual([45, 44, 41, 43, 42])
    const long = habitudesStats({ merchants: [...h.merchants, { ...h.merchants[0], merchant_id: 99, rank_amount: 11, rank_count: 6 }] })
    expect(classes(long, 'montant').map((m) => m.merchant_id)).not.toContain(99)
    expect(classes(long, 'nombre').map((m) => m.merchant_id)).toContain(99)
  })

  it('construit le lien vers les opérations de la fenêtre', () => {
    expect(lienOperations(habitudesStats(), 41)).toBe('from=2026-11-01&to=2026-11-26&merchant=41&direction=debit&date=purchase')
    expect(lienOperations(habitudesStats())).toBe('from=2026-11-01&to=2026-11-26&date=purchase')
    expect(lienOperations(habitudesStats({ mode: 'statement' }), 41)).toBe('from=2026-11-01&to=2026-11-26&merchant=41&direction=debit')
  })

  it('résume les jours et les semaines sans rien inventer quand il n’y a rien', () => {
    const h = habitudesStats()
    expect(forts(h.weekdays)).toEqual([5, 6])
    expect(plat(phraseJours(h.weekdays))).toBe('Vendredi et samedi concentrent 49 % de tes paiements par carte (538 € sur 1 103 €).')
    expect(plat(phraseSemaines(h.weeks))).toBe('C’est du 22 au 28 que tu dépenses le plus par jour : 61,60 € en moyenne.')
    expect(nomSemaine(h.weeks.buckets[0])).toBe('S1 · 1–7')
    const vide = { amount: 0, count: 0, buckets: h.weekdays.buckets.map((t) => ({ ...t, amount: 0, count: 0 })) }
    expect(phraseJours(vide)).toBe('')
    expect(phraseSemaines(vide)).toBe('')
    const un = { ...vide, amount: 500, buckets: vide.buckets.map((t) => (t.rank === 2 ? { ...t, amount: 500 } : t)) }
    expect(plat(phraseJours(un))).toBe('Mardi concentre 100 % de tes paiements par carte (5 € sur 5 €).')
  })

  it('dit pourquoi une opération est inhabituelle', () => {
    const [chez, dans] = habitudesStats().unusual.operations.map((o) => o.reason)
    expect(plat(fois(32))).toBe('3,2 ×')
    expect(plat(raisonDe(chez))).toBe('2,7 × ton panier habituel chez Livréo (15,32 €)')
    expect(plat(raisonDe(dans))).toBe('3,2 × ton achat habituel en Shopping (31,00 €)')
    expect(plat(raisonDe({ ...chez, ratio_x10: null, median: 0 }))).toBe('26,48 € au-dessus de ton panier habituel chez Livréo (0,00 €)')
    expect(methodeRaison(chez)).toBe('Comparée à la médiane de 9 autres achats chez ce commerçant.')
    expect(methodeRaison(dans)).toBe('Comparée à la médiane de 14 autres achats de cette catégorie.')
  })

  it('sépare actifs, arrêtés et écartés, et ne garde que les alertes non vues des actifs', () => {
    const r = recurrentsStats()
    expect([actifs(r).length, arretes(r).length, ecartes(r).length]).toEqual([6, 1, 1])
    expect(alertes(r).map((a) => [a.recurrent.name, a.alerte.kind])).toEqual([['Streamo', 'amount_changed'], ['Arcadia+', 'new']])
    expect(plat(phraseAlerte(alertes(r)[0]))).toBe('Depuis le 12 nov. 2026 : +2,00 € par mois, soit +24,00 € par an.')
    expect(plat(phraseAlerte(alertes(r)[1]))).toBe('2 échéances identiques depuis le 8 oct. 2026, la dernière le 8 nov. 2026 : 83,88 € par an.')
    const baisse = { recurrent: recurrent({ cadence: 'yearly' }), alerte: { ...alertes(r)[0].alerte, delta: -500, yearly_delta: -500 } }
    expect(plat(phraseAlerte(baisse))).toBe('Depuis le 12 nov. 2026 : −5,00 € par an, soit −5,00 € par an.')
  })

  it('décrit la cadence et ce qui manque à un historique court', () => {
    expect(cadenceDe(recurrent())).toBe('mensuel, vers le 5')
    expect(cadenceDe(recurrent({ cadence: 'weekly', last_date: '2026-11-23' }))).toBe('hebdomadaire, le lundi')
    expect(cadenceDe(recurrent({ cadence: 'yearly', last_date: '2026-03-20' }))).toBe('annuel, en mars')
    expect(cadenceDe(recurrent({ cadence: 'quarterly', last_date: '2026-10-01' }))).toBe('trimestriel, vers le 1er')
    expect(manque(recurrentsStats())).toBe('Il faut 13 mois pour un abonnement annuel.')
    expect(manque({ detectable: ['weekly', 'monthly', 'quarterly', 'yearly'] })).toBe('')
    expect(manque(recurrentsCourts())).toBe('Il faut 2 relevés qui se suivent pour un prélèvement mensuel (3 pour un paiement par carte), 2 trimestres pour un prélèvement trimestriel et 13 mois pour un abonnement annuel.')
    expect(plat(couverture(recurrentsCourts()))).toBe('1 relevé importé · 26 jours couverts')
  })
})
