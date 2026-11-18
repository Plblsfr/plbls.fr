import { describe, expect, it } from 'vitest'
import { ajouterJours, avancement, dateHeure, deMois, duree, ilYA, dureeRonde, jourCourt, jourLong, joursEntre, nombre, part, pluriel, valeur } from './formats'

describe('formats de Sablier', () => {
  it('écrit les durées en heures et minutes', () => {
    expect(duree(0)).toBe('0 min')
    expect(duree(30)).toBe('moins d’une minute')
    expect(duree(35 * 60)).toBe('35 min')
    expect(duree(3600)).toBe('1 h')
    expect(duree(31 * 3600 + 20 * 60)).toBe('31 h 20')
    expect(duree(22 * 3600 + 5 * 60)).toBe('22 h 05')
    expect(duree(412 * 3600 + 1800)).toBe('412 h')
    expect(dureeRonde(47 * 3600 + 1000)).toBe('47 h')
    expect(dureeRonde(20)).toBe('1 min')
  })

  it('sépare les milliers et accorde le pluriel', () => {
    expect(nombre(8640)).toBe('8 640')
    expect(nombre(642)).toBe('642')
    expect(nombre(1_234_567)).toBe('1 234 567')
    expect(pluriel(1, 'film', 'films')).toBe('1 film')
    expect(pluriel(4, 'film', 'films')).toBe('4 films')
  })

  it('écrit une valeur et un avancement selon leur unité', () => {
    expect(valeur(9 * 3600 + 1200, 'seconds')).toBe('9 h 20')
    expect(valeur(12, 'days')).toBe('12 jours')
    expect(valeur(642, 'count')).toBe('642')
    expect(avancement(38, 50, 'count')).toBe('38 / 50')
    expect(avancement(16, 20, 'days')).toBe('16 / 20 jours')
    expect(avancement(9300, 10_800, 'seconds')).toBe('2 h 35 / 3 h')
    expect(part(46, 100)).toBe(46)
    expect(part(1, 0)).toBe(0)
  })

  it('écrit les jours sans dépendre du fuseau', () => {
    expect(jourCourt('2026-11-08')).toBe('dim. 8 nov.')
    expect(jourLong('2024-03-03')).toBe('3 mars 2024')
    expect(jourLong('2026-11-01')).toBe('1er novembre 2026')
    expect(ajouterJours('2026-02-28', 1)).toBe('2026-03-01')
    expect(joursEntre('2026-11-01', '2026-11-18')).toBe(18)
    expect(deMois('2026-10-01')).toBe('d’octobre')
    expect(deMois('2026-11-01')).toBe('de novembre')
  })

  it('écrit une date avec son heure locale et une ancienneté', () => {
    expect(dateHeure(new Date(2026, 10, 12, 21, 14).toISOString())).toBe('12 nov. 2026 à 21:14')
    expect(dateHeure('pas une date')).toBe('')
    expect(ilYA('2026-11-18', '2026-11-18')).toBe('aujourd’hui')
    expect(ilYA('2026-11-17', '2026-11-18')).toBe('hier')
    expect(ilYA('2026-11-12', '2026-11-18')).toBe('il y a 6 jours')
    expect(ilYA('2026-06-12', '2026-11-18')).toBe('il y a 5 mois')
    expect(ilYA('2024-03-03', '2026-11-18')).toBe('il y a 2 ans')
    expect(ilYA(null, '2026-11-18')).toBe('jamais')
  })
})
