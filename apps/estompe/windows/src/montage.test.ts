import { describe, expect, it } from 'vitest'
import {
  actionClavier, annuler, appliquer, bornes, caler, couper, duree, dureeFinale, formaterTemps, gain, gardes, historique, imageDe, images, initial, lireProjet, lireTemps, nomTravail, pas, pics,
  regler, restaurer, retablir, retirer, rogner, segmentA, sourceDe, suite, tempsImage, traceOnde, versMontage, versSource, VERSION, type Montage, type Source,
} from './montage'

const S: Source = { nom: 'Jeu.mp4', dureeS: 10, ips: 60, pistes: 2 }
const plages = (m: Montage) => m.segments.map((s) => [s.debut, s.fin, s.garde])
const coupes = (m: Montage, ...t: number[]) => t.reduce((x, c) => couper(x, S, c), m)
const trois = () => retirer(retirer(coupes(rogner(rogner(initial(S), S, 'debut', 0.5), S, 'fin', 9.5), 2, 3, 5, 7), 1), 3)

describe('modèle de montage', () => {
  it('part du clip entier, volumes à 100 %', () => {
    const m = initial(S)
    expect(m).toEqual({ version: VERSION, segments: [{ source: 'Jeu.mp4', debut: 0, fin: 10, garde: true }], volumes: { 'Jeu.mp4': [{ volume: 1, muet: false }, { volume: 1, muet: false }] } })
    expect([images(S), duree(S), dureeFinale(m)]).toEqual([600, 10, 10])
    expect(sourceDe({ source: 'a.mp4', dureeS: 3, ips: 30, pistes: [{ nom: 'Jeu', chemin: 'p' }] })).toEqual({ nom: 'a.mp4', dureeS: 3, ips: 30, pistes: 1 })
    expect(duree({ ...S, dureeS: 10.004 })).toBe(10)
  })
  it('rogne le début et la fin à l’image près, dans les bornes', () => {
    const m = rogner(rogner(initial(S), S, 'debut', 1.004), S, 'fin', 8.26)
    expect(bornes(m)).toEqual({ debut: 1, fin: 496 / 60 })
    expect(dureeFinale(m)).toBeCloseTo(436 / 60, 9)
    expect(bornes(rogner(m, S, 'debut', 99))).toEqual({ debut: 495 / 60, fin: 496 / 60 })
    expect(bornes(rogner(m, S, 'fin', -3))).toEqual({ debut: 1, fin: 61 / 60 })
    expect(bornes(rogner(rogner(m, S, 'debut', -5), S, 'fin', 50))).toEqual({ debut: 0, fin: 10 })
    expect(rogner(m, S, 'debut', 1)).toBe(m)
    expect(rogner(m, S, 'fin', 496 / 60)).toBe(m)
    expect(bornes(rogner(m, S, 'debut', Number.NaN)).debut).toBe(0)
  })
  it('rogner au-delà d’un point de coupe retire les segments dépassés, élargir rend un segment gardé', () => {
    const m = trois()
    expect(plages(m)).toEqual([[0.5, 2, true], [2, 3, false], [3, 5, true], [5, 7, false], [7, 9.5, true]])
    expect(plages(rogner(m, S, 'debut', 4))).toEqual([[4, 5, true], [5, 7, false], [7, 9.5, true]])
    expect(plages(rogner(m, S, 'fin', 6))).toEqual([[0.5, 2, true], [2, 3, false], [3, 5, true], [5, 6, false]])
    const retireEnTete = rogner(m, S, 'debut', 2.5)
    expect(plages(rogner(retireEnTete, S, 'debut', 1))).toEqual([[1, 2.5, true], [2.5, 3, false], [3, 5, true], [5, 7, false], [7, 9.5, true]])
    expect(plages(rogner(rogner(m, S, 'fin', 6), S, 'fin', 8))).toEqual([[0.5, 2, true], [2, 3, false], [3, 5, true], [5, 6, false], [6, 8, true]])
    expect(plages(rogner(m, S, 'fin', 10)).at(-1)).toEqual([7, 10, true])
  })
  it('pose des points de coupe, retire et restaure un segment', () => {
    const m = coupes(initial(S), 2.004, 5)
    expect(plages(m)).toEqual([[0, 2, true], [2, 5, true], [5, 10, true]])
    expect(couper(m, S, 2)).toBe(m)
    expect(couper(m, S, 0)).toBe(m)
    expect(couper(m, S, 10)).toBe(m)
    expect(couper(m, S, 42)).toBe(m)
    const r = retirer(m, 1)
    expect(plages(r)).toEqual([[0, 2, true], [2, 5, false], [5, 10, true]])
    expect(retirer(r, 1)).toBe(r)
    expect(retirer(r, 9)).toBe(r)
    expect(plages(couper(r, S, 3))).toEqual([[0, 2, true], [2, 3, false], [3, 5, false], [5, 10, true]])
    expect(restaurer(r, 1)).toEqual(m)
    expect([segmentA(r, 0), segmentA(r, 2), segmentA(r, 4.99), segmentA(r, 10), segmentA(rogner(r, S, 'debut', 1), 0.5)]).toEqual([0, 1, 1, -1, -1])
  })
  it('recolle les segments gardés dans l’ordre et donne la durée finale', () => {
    const m = trois()
    expect(gardes(m).map((s) => [s.debut, s.fin])).toEqual([[0.5, 2], [3, 5], [7, 9.5]])
    expect(dureeFinale(m)).toBe(6)
    expect(dureeFinale(retirer(retirer(retirer(m, 0), 2), 4))).toBe(0)
  })
  it('convertit le temps du montage et le temps de la source', () => {
    const m = trois()
    expect([0, 0.5, 1, 2, 2.5, 3, 4, 6, 7, 9.5, 10].map((t) => versMontage(m, t))).toEqual([0, 0, 0.5, 1.5, 1.5, 1.5, 2.5, 3.5, 3.5, 6, 6])
    expect([0, 1, 1.5, 2.5, 3.5, 5.9, 6, 99, -1].map((t) => versSource(m, t))).toEqual([0.5, 1.5, 3, 4, 7, 9.4, 9.5, 9.5, 0.5])
    for (const t of [0.5, 1.25, 3, 4.5, 7, 9]) expect(versSource(m, versMontage(m, t))).toBeCloseTo(t, 9)
    expect(versSource(retirer(initial(S), 0), 3)).toBe(0)
  })
  it('saute les passages retirés à la lecture et s’arrête à la fin', () => {
    const m = trois()
    expect(suite(m, tempsImage(60, 60), 60)).toBeNull()
    expect(suite(m, 0, 60)).toEqual({ aller: 0.5 })
    expect(suite(m, tempsImage(119, 60), 60)).toBeNull()
    expect(suite(m, 2, 60)).toEqual({ aller: 3 })
    expect(suite(m, tempsImage(150, 60), 60)).toEqual({ aller: 3 })
    expect(suite(m, 3, 60)).toBeNull()
    expect(suite(m, 6.2, 60)).toEqual({ aller: 7 })
    expect(suite(m, 9.5, 60)).toEqual({ fin: true })
    expect(suite(m, 9.99, 60)).toEqual({ fin: true })
  })
  it('avance et recule image par image selon la cadence', () => {
    expect([imageDe(0, 60), imageDe(1 / 60, 60), imageDe(tempsImage(7, 60), 60), imageDe(2, 30), imageDe(119.9999 / 60, 60)]).toEqual([0, 1, 7, 60, 120])
    expect(pas(1, 1, S)).toBeCloseTo(61 / 60, 12)
    expect(pas(tempsImage(60, 60), -1, S)).toBeCloseTo(59 / 60, 12)
    expect(pas(0, -1, S)).toBe(0)
    expect(pas(10, 1, S)).toBe(599 / 60)
    expect(pas(1, 1, { ...S, ips: 30 })).toBeCloseTo(31 / 30, 12)
    expect([caler(1.004, S), caler(-1, S), caler(99, S), caler(1.02, { ...S, ips: 30 })]).toEqual([1, 0, 10, 31 / 30])
  })
  it('règle le volume de 0 à 200 % et le muet par piste', () => {
    const m = regler(regler(initial(S), S, 0, { volume: 0.5 }), S, 1, { muet: true })
    expect(m.volumes['Jeu.mp4']).toEqual([{ volume: 0.5, muet: false }, { volume: 1, muet: true }])
    expect([gain(m, S, 0), gain(m, S, 1), gain(m, S, 5)]).toEqual([0.5, 0, 1])
    expect(gain(regler(m, S, 0, { volume: 9 }), S, 0)).toBe(2)
    expect(gain(regler(m, S, 0, { volume: -1 }), S, 0)).toBe(0)
    expect(regler(m, S, 0, { volume: 0.5 })).toBe(m)
    expect(regler(m, S, 2, { volume: 0.5 })).toBe(m)
    expect(regler(m, S, 0, { volume: Number.NaN })).toBe(m)
    expect(regler(m, S, 1, { muet: false }).volumes['Jeu.mp4'][1]).toEqual({ volume: 1, muet: false })
  })
})

describe('annuler et rétablir', () => {
  it('restaure exactement chaque état sur dix opérations', () => {
    const operations: ((m: Montage) => Montage)[] = [
      (m) => rogner(m, S, 'debut', 0.5), (m) => rogner(m, S, 'fin', 9.5), (m) => couper(m, S, 2), (m) => couper(m, S, 3), (m) => retirer(m, 1),
      (m) => regler(m, S, 0, { volume: 0.5 }), (m) => regler(m, S, 1, { muet: true }), (m) => couper(m, S, 5), (m) => restaurer(m, 1), (m) => rogner(m, S, 'debut', 2.5),
    ]
    const etats = [initial(S)]
    let h = historique(etats[0])
    for (const op of operations) {
      h = appliquer(h, op(h.present))
      etats.push(structuredClone(h.present))
    }
    expect(h.passe).toHaveLength(10)
    expect(new Set(etats.map((e) => JSON.stringify(e))).size).toBe(11)
    for (let i = 9; i >= 0; i--) {
      h = annuler(h)
      expect(h.present).toEqual(etats[i])
    }
    expect(annuler(h)).toBe(h)
    expect(h.futur).toHaveLength(10)
    for (let i = 1; i <= 10; i++) {
      h = retablir(h)
      expect(h.present).toEqual(etats[i])
    }
    expect(retablir(h)).toBe(h)
    expect(h.passe).toEqual(etats.slice(0, 10))
  })
  it('ignore une opération sans effet et oublie le futur après une nouvelle opération', () => {
    let h = appliquer(historique(initial(S)), couper(initial(S), S, 4))
    expect(appliquer(h, couper(h.present, S, 4))).toBe(h)
    h = annuler(h)
    h = appliquer(h, couper(h.present, S, 6))
    expect([h.passe.length, h.futur.length, h.present.segments[0].fin]).toEqual([1, 0, 6])
    for (let i = 0; i < 250; i++) h = appliquer(h, regler(h.present, S, 0, { volume: (i % 2) + 0.5 }))
    expect(h.passe).toHaveLength(200)
  })
})

describe('projet enregistré', () => {
  it('relit un projet tel qu’il a été écrit', () => {
    const m = regler(trois(), S, 0, { volume: 0.5 })
    expect(lireProjet(JSON.parse(JSON.stringify(m)), S)).toEqual(m)
  })
  it('ramène la source au nom actuel du clip et les temps sur les images', () => {
    const m = lireProjet({ version: 1, segments: [{ source: 'Ancien nom.mp4', debut: 0.504, fin: 2 }, { source: 'Ancien nom.mp4', debut: 2, fin: 99, garde: false }], volumes: { 'Ancien nom.mp4': [{ volume: 5, muet: true }] } }, S)
    expect(plages(m)).toEqual([[0.5, 2, true], [2, 10, false]])
    expect(m.segments.every((s) => s.source === 'Jeu.mp4')).toBe(true)
    expect(m.volumes).toEqual({ 'Jeu.mp4': [{ volume: 2, muet: true }, { volume: 1, muet: false }] })
  })
  it('repart du clip entier si le projet est illisible ou d’une autre version', () => {
    const seg = (debut: number, fin: number) => ({ source: 'Jeu.mp4', debut, fin, garde: true })
    for (const brut of [null, 'x', 3, {}, { version: 2, segments: [seg(0, 1)] }, { version: 1, segments: [] }, { version: 1, segments: [seg(2, 1)] }, { version: 1, segments: [seg(0, 1), seg(3, 4)] }, { version: 1, segments: [seg(0, 2), seg(1, 3)] }, { version: 1, segments: [null] }, { version: 1, segments: [{ debut: '0', fin: 1 }] }, { version: 1, segments: [seg(20, 30)] }]) {
      expect(lireProjet(brut, S)).toEqual(initial(S))
    }
  })
})

describe('affichage et clavier de l’éditeur', () => {
  it('écrit et lit un temps précis', () => {
    expect([formaterTemps(0), formaterTemps(3.254), formaterTemps(62.5), formaterTemps(599.999), formaterTemps(-1)]).toEqual(['0:00,00', '0:03,25', '1:02,50', '10:00,00', '0:00,00'])
    expect([lireTemps('1:02,5'), lireTemps('62.5'), lireTemps(' 0:03 '), lireTemps('1:00:01'), lireTemps('7')]).toEqual([62.5, 62.5, 3, 3601, 7])
    expect([lireTemps(''), lireTemps('abc'), lireTemps('1:'), lireTemps('-3'), lireTemps('1:2:3:4'), lireTemps('1e3')]).toEqual([null, null, null, null, null, null])
  })
  it('calcule la forme d’onde d’une piste', () => {
    const gauche = Float32Array.from({ length: 6400 }, (_, i) => (i < 3200 ? 0.25 : -0.5))
    const droite = Float32Array.from({ length: 6400 }, (_, i) => (i >= 4800 ? 1.5 : 0))
    expect(pics([gauche, droite], 4)).toEqual([0.25, 0.25, 0.5, 1])
    expect(pics([], 3)).toEqual([0, 0, 0])
    expect(pics([new Float32Array(0)], 2)).toEqual([0, 0])
    expect(traceOnde([0, 1])).toBe('M0.5 49.5V50.5M1.5 0.0V100.0')
    expect(traceOnde([0, 0.125])).toBe(traceOnde([0, 1]))
    expect(traceOnde([0, 0])).toBe('M0.5 49.5V50.5M1.5 49.5V50.5')
  })
  it('associe les raccourcis aux actions, sauf quand un champ a le focus', () => {
    const t = (key: string, p: Partial<KeyboardEvent> = {}) => ({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...p })
    expect([' ', 'ArrowLeft', 'ArrowRight', 'c', 'C', 'Delete'].map((k) => actionClavier(t(k), false))).toEqual(['lecture', 'precedente', 'suivante', 'couper', 'couper', 'basculer'])
    expect([actionClavier(t('z', { ctrlKey: true }), false), actionClavier(t('y', { ctrlKey: true }), false), actionClavier(t('Z', { ctrlKey: true, shiftKey: true }), false)]).toEqual(['annuler', 'retablir', 'retablir'])
    expect([actionClavier(t('c', { ctrlKey: true }), false), actionClavier(t('z'), false), actionClavier(t('c', { altKey: true }), false), actionClavier(t('Enter'), false)]).toEqual([null, null, null, null])
    for (const k of [' ', 'ArrowLeft', 'c', 'Delete']) expect(actionClavier(t(k), true)).toBeNull()
    expect(actionClavier(t('z', { ctrlKey: true }), true)).toBeNull()
  })
  it('nomme un export en cours', () => {
    expect(nomTravail({ nom: 'Jeu', logiciel: false })).toBe('Export de « Jeu »')
    expect(nomTravail({ nom: 'Jeu', logiciel: true })).toBe('Export de « Jeu » (encodeur logiciel)')
  })
})
