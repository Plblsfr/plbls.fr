import { describe, expect, it } from 'vitest'
import { dateCourte, dateLongue, ecart, euros, eurosSignes, instant, jourLong, moisLong, montant, nombre, periode, pluriel, signe } from './formats'

describe('montants en euros', () => {
  it('écrit des centimes entiers à la française', () => {
    expect(euros(123_456)).toBe('1\u202f234,56\u00a0€')
    expect(euros(0)).toBe('0,00\u00a0€')
    expect(euros(5)).toBe('0,05\u00a0€')
    expect(euros(990)).toBe('9,90\u00a0€')
    expect(euros(100_000_000)).toBe('1\u202f000\u202f000,00\u00a0€')
    expect(euros(-7280)).toBe('−72,80\u00a0€')
  })

  it('ne passe jamais par des flottants : les grands montants restent exacts', () => {
    expect(euros(900_719_925_474_099)).toBe('9\u202f007\u202f199\u202f254\u202f740,99\u00a0€')
    expect(euros(1005)).toBe('10,05\u00a0€')
    expect(euros(2990)).toBe('29,90\u00a0€')
  })

  it('refuse un montant qui n’est pas un entier', () => {
    expect(() => euros(12.5)).toThrow(RangeError)
    expect(() => euros(Number.NaN)).toThrow(RangeError)
  })

  it('signe les débits et les crédits', () => {
    expect(montant(3120, 'debit')).toBe('−31,20\u00a0€')
    expect(montant(264_000, 'credit')).toBe('+2\u202f640,00\u00a0€')
    expect(eurosSignes(0)).toBe('+0,00\u00a0€')
    expect(signe(3120, 'debit')).toBe(-3120)
    expect(signe(3120, 'credit')).toBe(3120)
  })

  it('écrit un écart de contrôle', () => {
    expect(ecart(-7280)).toBe('−72,80\u00a0€')
    expect(ecart(7280)).toBe('+72,80\u00a0€')
    expect(ecart(0)).toBe('=\u00a00,00\u00a0€')
  })

  it('sépare les milliers des nombres et accorde', () => {
    expect(nombre(84)).toBe('84')
    expect(nombre(12_345)).toBe('12\u202f345')
    expect(pluriel(1, 'opération', 'opérations')).toBe('1 opération')
    expect(pluriel(0, 'opération', 'opérations')).toBe('0 opération')
    expect(pluriel(1284, 'opération', 'opérations')).toBe('1\u202f284 opérations')
  })
})

describe('dates', () => {
  it('nomme un jour, avec l’année quand elle diffère', () => {
    expect(jourLong('2026-11-26')).toBe('Jeudi 26 novembre')
    expect(jourLong('2026-11-26', 2026)).toBe('Jeudi 26 novembre')
    expect(jourLong('2025-12-31', 2026)).toBe('Mercredi 31 décembre 2025')
    expect(jourLong('2026-11-01')).toBe('Dimanche 1er novembre')
  })

  it('écrit les dates courtes et longues', () => {
    expect(dateCourte('2026-11-24')).toBe('24 nov. 2026')
    expect(dateCourte('2026-08-01')).toBe('1er août 2026')
    expect(dateLongue('2026-11-22')).toBe('dimanche 22 nov. 2026')
    expect(moisLong('2026-11-26')).toBe('novembre 2026')
  })

  it('ne décale pas le jour selon le fuseau', () => {
    expect(dateCourte('2026-01-01')).toBe('1er janv. 2026')
    expect(dateCourte('2026-12-31')).toBe('31 déc. 2026')
  })

  it('rend une date illisible telle quelle', () => {
    expect(dateCourte('26/11/2026')).toBe('26/11/2026')
    expect(dateCourte('2026-02-30')).toBe('2026-02-30')
    expect(jourLong('')).toBe('')
    expect(instant('pas une date')).toBe('pas une date')
  })

  it('résume une période', () => {
    expect(periode('2026-11-01', '2026-11-26')).toBe('Du 1er au 26 nov. 2026')
    expect(periode('2026-10-31', '2026-11-26')).toBe('Du 31 oct. au 26 nov. 2026')
    expect(periode('2025-12-28', '2026-01-27')).toBe('Du 28 déc. 2025 au 27 janv. 2026')
    expect(periode('2026-11-26', '2026-11-26')).toBe('Le 26 nov. 2026')
  })

  it('date un import dans le fuseau de l’appareil', () => {
    expect(instant(new Date(2026, 9, 2, 14, 5).toISOString())).toBe('2 oct. 2026')
    expect(instant(new Date(2026, 10, 1, 8, 0).toISOString())).toBe('1er nov. 2026')
  })
})
