import { describe, expect, it } from 'vitest'
import { borner, contient, debutDe, finDe, intitule, libelle, libelleRecap, peutAvancer, peutReculer, precedente, suivante, titre } from './periodes'

const AUJ = '2026-11-18'
const PREMIER = '2026-09-03'

describe('périodes de Sablier', () => {
  it('borne une semaine du lundi au dimanche, un mois et une année', () => {
    expect([debutDe('week', AUJ), finDe('week', AUJ)]).toEqual(['2026-11-16', '2026-11-22'])
    expect([debutDe('week', '2026-11-22'), finDe('week', '2026-11-16')]).toEqual(['2026-11-16', '2026-11-22'])
    expect([debutDe('month', AUJ), finDe('month', AUJ)]).toEqual(['2026-11-01', '2026-11-30'])
    expect([debutDe('month', '2024-02-10'), finDe('month', '2024-02-10')]).toEqual(['2024-02-01', '2024-02-29'])
    expect([debutDe('year', AUJ), finDe('year', AUJ)]).toEqual(['2026-01-01', '2026-12-31'])
  })

  it('C3 : la navigation ne dépasse pas la période en cours', () => {
    expect(peutAvancer('month', AUJ, AUJ)).toBe(false)
    expect(suivante('month', AUJ, AUJ)).toBe(AUJ)
    expect(peutAvancer('month', '2026-10-31', AUJ)).toBe(true)
    expect(suivante('month', '2026-10-31', AUJ)).toBe(AUJ)
    expect(suivante('month', '2026-09-30', AUJ)).toBe('2026-10-31')
    expect(peutAvancer('week', '2026-11-22', AUJ)).toBe(false)
    expect(peutAvancer('all', '2026-01-01', AUJ)).toBe(false)
  })

  it('C3 : la navigation ne remonte pas avant le premier jour d’activité', () => {
    expect(precedente('month', AUJ, PREMIER)).toBe('2026-10-31')
    expect(precedente('month', '2026-10-31', PREMIER)).toBe('2026-09-30')
    expect(peutReculer('month', '2026-09-30', PREMIER)).toBe(false)
    expect(precedente('month', '2026-09-30', PREMIER)).toBe('2026-09-30')
    expect(peutReculer('week', '2026-09-06', PREMIER)).toBe(false)
    expect(peutReculer('week', '2026-09-07', PREMIER)).toBe(true)
    expect(peutReculer('year', AUJ, PREMIER)).toBe(false)
    expect(peutReculer('year', AUJ, '2024-03-03')).toBe(true)
    expect(peutReculer('month', AUJ, null)).toBe(false)
    expect(peutReculer('all', AUJ, PREMIER)).toBe(false)
  })

  it('ramène une date dans les bornes', () => {
    expect(borner('2027-01-01', PREMIER, AUJ)).toBe(AUJ)
    expect(borner('2026-01-01', PREMIER, AUJ)).toBe(PREMIER)
    expect(borner('2026-10-10', PREMIER, AUJ)).toBe('2026-10-10')
  })

  it('nomme les périodes', () => {
    expect(libelle('month', AUJ)).toBe('Novembre 2026')
    expect(libelle('week', AUJ)).toBe('16 – 22 nov. 2026')
    expect(libelle('week', '2026-12-01')).toBe('30 nov. – 6 déc. 2026')
    expect(libelle('year', AUJ)).toBe('2026')
    expect(titre('month', AUJ)).toEqual({ avant: 'Ton mois de ', fort: 'novembre' })
    expect(titre('month', '2026-10-05')).toEqual({ avant: 'Ton mois d’', fort: 'octobre' })
    expect(titre('week', AUJ)).toEqual({ avant: 'Ta ', fort: 'semaine' })
    expect(intitule('month', AUJ, AUJ)).toBe('Ce mois-ci')
    expect(intitule('month', '2026-10-31', AUJ)).toBe('Octobre 2026')
    expect(intitule('all', AUJ, AUJ)).toBe('Depuis le début')
    expect(libelleRecap('month', '2026-10')).toBe('Octobre 2026')
    expect(libelleRecap('year', '2025')).toBe('2025')
  })

  it('dit si un jour tombe dans une période', () => {
    expect(contient('month', AUJ, '2026-11-14')).toBe(true)
    expect(contient('month', AUJ, '2026-10-31')).toBe(false)
    expect(contient('all', AUJ, '2020-01-01')).toBe(true)
  })
})
