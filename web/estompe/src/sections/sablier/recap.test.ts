import { describe, expect, it } from 'vitest'
import { entracte, messagerie, recap, recaps, sillon, usage } from './essais'
import { ecransDuRecap, prochainRecap, rangerRecaps, titreRecap } from './recap'
import type { NomSalon } from './stats'

const nom: NomSalon = (_s, salon) => (salon === 'c1' ? 'général' : null)
const texte = (m: { texte: string }[]) => m.map((x) => x.texte).join('')

describe('découpage du récap en écrans', () => {
  it('rédige un écran par domaine à partir des totaux et des faits marquants', () => {
    const e = ecransDuRecap(recap(), nom)
    expect(e.map((x) => x.cle)).toEqual(['ouverture', 'usage', 'entracte', 'sillon', 'messaging', 'trophees'])
    expect(texte(e[0].grand)).toBe('Ton mois d’octobre')
    expect(e[0].texte).toBe('Six écrans. Touche à droite pour avancer, à gauche pour revenir.')
    expect(texte(e[1].grand)).toBe('31 h')
    expect(texte(e[1].titre)).toBe('de temps actif, sur 16 jours du mois.')
    expect(e[1].texte).toBe('Ta plus longue journée : samedi 17, avec 4 h 10. Surtout sur Windows.')
    expect(texte(e[2].grand)).toBe('23 épisodes')
    expect(texte(e[2].titre)).toBe('et 4 films. Tu as terminé 1 série.')
    expect(e[2].texte).toContain('Tu as enchaîné 5 épisodes d’un coup.')
    expect(texte(e[3].titre)).toBe('d’écoute, pour 148 titres différents.')
    expect(texte(e[4].grand)).toBe('642 messages')
    expect(texte(e[4].titre)).toBe('dont 96 samedi 31, ta journée la plus bavarde.')
    expect(e[4].texte).toBe('# général reste ton salon. 9 h 20 en vocal, avec un appel de 2 h 35. Ton emoji du mois : 🎉')
    expect(texte(e[5].grand)).toBe('1 trophée')
    expect(texte(e[5].titre)).toBe('débloqué en octobre, et une série de 41 jours.')
    expect(e[5].trophees).toHaveLength(1)
  })

  it('saute un écran sans rien à dire', () => {
    const pauvre = recap({ entracte: undefined, sillon: (({ access: _a, ...s }) => s)(sillon()), messaging: messagerie(), trophies: [], streak: { global: 1 }, highlights: [] })
    expect(ecransDuRecap(pauvre, nom).map((x) => x.cle)).toEqual(['ouverture', 'usage'])
    const vide = recap({ usage: usage(), entracte: (({ access: _a, ...s }) => s)(entracte()), sillon: undefined, messaging: messagerie(), trophies: [], streak: {}, highlights: [] })
    const e = ecransDuRecap(vide, nom)
    expect(e.map((x) => x.cle)).toEqual(['ouverture'])
    expect(e[0].texte).toBe('Peu de choses à raconter sur cette période.')
  })

  it('tait le nom d’un salon supprimé et garde le reste', () => {
    const e = ecransDuRecap(recap(), () => null)
    expect(e[4].texte).toBe('9 h 20 en vocal, avec un appel de 2 h 35. Ton emoji du mois : 🎉')
  })

  it('récap de l’année : titres et dates de l’année', () => {
    const e = ecransDuRecap(recap({ kind: 'year', period: '2026', from: '2026-01-01', to: '2026-12-31' }), nom)
    expect(texte(e[0].grand)).toBe('Ton année 2026')
    expect(texte(e[1].titre)).toBe('de temps actif, sur 16 jours de l’année.')
    expect(texte(e[4].titre)).toBe('dont 96 le 31 octobre, ta journée la plus bavarde.')
    expect(titreRecap(recap({ kind: 'year', period: '2026' }))).toBe('Récap · année 2026')
    expect(titreRecap(recap())).toBe('Récap · octobre 2026')
  })

  it('limite les médailles et compte le reste', () => {
    const t = { name: 'T', description: '', icon: 'film', tier: 1, unlocked_at: '2026-10-02' }
    const e = ecransDuRecap(recap({ trophies: Array.from({ length: 9 }, () => t) }), nom).at(-1)!
    expect(e.trophees).toHaveLength(4)
    expect(e.reste).toBe(5)
  })
})

describe('liste des récaps', () => {
  it('met en avant le récap non vu, range les mois avec leurs trous, et les années', () => {
    const r = rangerRecaps(recaps())
    expect(r.nouveau?.id).toBe(12)
    expect(r.mois.map((c) => [c.label, c.recap?.id ?? null])).toEqual([['Septembre 2026', null], ['Août 2026', 9]])
    expect(r.annees.map((c) => c.label)).toEqual(['2025'])
    expect(rangerRecaps([])).toEqual({ nouveau: null, mois: [], annees: [] })
  })

  it('annonce le prochain récap', () => {
    expect(prochainRecap('2026-11-18')).toBe('1er décembre')
    expect(prochainRecap('2026-12-31')).toBe('1er janvier')
  })
})
