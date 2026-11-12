import { describe, expect, it } from 'vitest'
import {
  actionClavier, ajouterClip, ajouterTexte, annuler, appliquer, axe, bloc, bornerCadre, bornes, cadreAu, cadrePixels, cadencesPossibles, caler, clipsDe, CLIPS_MAX, corps, couper, deplacer, deplacerClip, dessinerTexte,
  dimensionsSortie, duree, dureeFinale, formatDe, formaterTemps, gain, gardes, hauteursPossibles, historique, imageDe, images, initial, inscrire, lieu, lireProjet, lireTemps, nomTravail, pas, pics, policeCss,
  position, recadrer, refus, regler, reglerSegment, reglerSortie, reglerTexte, restaurer, retablir, retirer, retirerClip, retirerTexte, rogner, saccade, sourceDe, sourcesDe, suite, tempsImage, textesA, toile,
  traceOnde, versAxe, versMontage, VERSION, type Cadre, type Montage, type Pinceau, type Source, type Texte,
} from './montage'

const S: Source = { nom: 'Jeu.mp4', dureeS: 10, ips: 60, pistes: 2, largeur: 1920, hauteur: 1080 }
const H: Source = { nom: 'Hevc.mp4', dureeS: 8, ips: 144, pistes: 1, largeur: 2560, hauteur: 1440 }
const C: Source = { nom: 'Carre.mp4', dureeS: 5, ips: 30, pistes: 0, largeur: 1000, hauteur: 1000 }
const SRC = { [S.nom]: S, [H.nom]: H, [C.nom]: C }
const plages = (m: Montage) => m.segments.map((s) => [s.debut, s.fin, s.garde])
const ordre = (m: Montage) => m.segments.map((s) => `${s.source[0]}${s.debut}`).join(' ')
const coupes = (m: Montage, ...t: number[]) => t.reduce((x, c) => couper(x, S, c), m)
const trois = () => retirer(retirer(coupes(rogner(rogner(initial(S), S, 'debut', 0.5), S, 'fin', 9.5), 2, 3, 5, 7), 1), 3)
const tous = () => ajouterClip(ajouterClip(initial(S), H), C)

describe('modèle de montage', () => {
  it('part du clip entier, volumes à 100 %', () => {
    const m = initial(S)
    expect(m).toEqual({
      version: VERSION, nom: 'Jeu', segments: [{ source: 'Jeu.mp4', debut: 0, fin: 10, garde: true, vitesse: 1, muet: false }], volumes: { 'Jeu.mp4': [{ volume: 1, muet: false }, { volume: 1, muet: false }] },
      textes: [], cadre: { x: 0, y: 0, l: 1, h: 1, rapport: 'libre' }, sortie: { hauteur: null, ips: null },
    })
    expect([VERSION, images(S), duree(S), dureeFinale(m)]).toEqual([2, 600, 10, 10])
    expect(sourceDe({ source: 'a.mp4', dureeS: 3, ips: 30, pistes: [{ nom: 'Jeu', chemin: 'p' }], largeur: 640, hauteur: 360 })).toEqual({ nom: 'a.mp4', dureeS: 3, ips: 30, pistes: 1, largeur: 640, hauteur: 360 })
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
    expect([couper(m, S, 3, 0), couper(m, S, 3, 9), couper(m, H, 3)]).toEqual([m, m, m])
    expect(plages(couper(m, S, 3, 1))).toEqual([[0, 2, true], [2, 3, true], [3, 5, true], [5, 10, true]])
    const r = retirer(m, 1)
    expect(plages(r)).toEqual([[0, 2, true], [2, 5, false], [5, 10, true]])
    expect(retirer(r, 1)).toBe(r)
    expect(retirer(r, 9)).toBe(r)
    expect(plages(couper(r, S, 3))).toEqual([[0, 2, true], [2, 3, false], [3, 5, false], [5, 10, true]])
    expect(restaurer(r, 1)).toEqual(m)
  })
  it('recolle les segments gardés dans l’ordre et donne la durée finale', () => {
    const m = trois()
    expect(gardes(m).map((s) => [s.debut, s.fin])).toEqual([[0.5, 2], [3, 5], [7, 9.5]])
    expect(dureeFinale(m)).toBe(6)
    expect(dureeFinale(retirer(retirer(retirer(m, 0), 2), 4))).toBe(0)
  })
  it('convertit la position sur la ligne de temps, le lieu dans un clip et le temps du montage', () => {
    const m = trois()
    expect(axe(m)).toEqual({ debuts: [0, 1.5, 2.5, 4.5, 6.5], total: 9 })
    expect([0, 1, 1.5, 2, 2.5, 6.5, 8.99, 9, 99, -1].map((u) => lieu(m, u))).toEqual([{ i: 0, t: 0.5 }, { i: 0, t: 1.5 }, { i: 1, t: 2 }, { i: 1, t: 2.5 }, { i: 2, t: 3 }, { i: 4, t: 7 }, { i: 4, t: 9.49 }, { i: 4, t: 9.5 }, { i: 4, t: 9.5 }, { i: 0, t: 0.5 }])
    for (const u of [0, 0.75, 1.5, 3, 4.5, 8]) expect(position(m, lieu(m, u))).toBeCloseTo(u, 9)
    expect([0, 0.5, 1.5, 2, 2.5, 3.5, 4.5, 6, 6.5, 9, 99].map((u) => versMontage(m, u))).toEqual([0, 0.5, 1.5, 1.5, 1.5, 2.5, 3.5, 3.5, 3.5, 6, 6])
    expect([0, 1, 1.5, 2.5, 3.5, 5.9, 6, 99, -1].map((t) => versAxe(m, t))).toEqual([0, 1, 2.5, 3.5, 6.5, 8.9, 9, 9, 0])
    for (const u of [0, 1.25, 2.5, 4, 6.5, 8]) expect(versAxe(m, versMontage(m, u))).toBeCloseTo(u, 9)
    expect(versAxe(retirer(initial(S), 0), 3)).toBe(0)
  })
  it('saute les passages retirés à la lecture et s’arrête à la fin', () => {
    const m = trois()
    expect(suite(m, { i: 0, t: tempsImage(60, 60) }, SRC)).toBeNull()
    expect(suite(m, { i: 0, t: 0 }, SRC)).toEqual({ aller: { i: 0, t: 0.5 } })
    expect(suite(m, { i: 0, t: tempsImage(119, 60) }, SRC)).toBeNull()
    expect(suite(m, { i: 0, t: 2 }, SRC)).toEqual({ aller: { i: 2, t: 3 } })
    expect(suite(m, { i: 1, t: tempsImage(150, 60) }, SRC)).toEqual({ aller: { i: 2, t: 3 } })
    expect(suite(m, { i: 2, t: 3 }, SRC)).toBeNull()
    expect(suite(m, { i: 3, t: 6.2 }, SRC)).toEqual({ aller: { i: 4, t: 7 } })
    expect(suite(m, { i: 4, t: 9.5 }, SRC)).toEqual({ fin: true })
    expect(suite(m, { i: 4, t: 9.99 }, SRC)).toEqual({ fin: true })
    const deux = deplacer(tous(), 2, 0)
    expect(suite(deux, { i: 0, t: 5 }, SRC)).toEqual({ aller: { i: 1, t: 0 } })
    expect(suite(retirer(deux, 1), { i: 0, t: 5 }, SRC)).toEqual({ aller: { i: 2, t: 0 } })
  })
  it('avance et recule image par image selon la cadence, d’un segment à l’autre', () => {
    expect([imageDe(0, 60), imageDe(1 / 60, 60), imageDe(tempsImage(7, 60), 60), imageDe(2, 30), imageDe(119.9999 / 60, 60)]).toEqual([0, 1, 7, 60, 120])
    const m = deplacer(tous(), 2, 1)
    expect(ordre(m)).toBe('J0 C0 H0')
    expect(pas(m, { i: 0, t: 1 }, 1, SRC)).toEqual({ i: 0, t: 61 / 60 })
    expect(pas(m, { i: 0, t: tempsImage(60, 60) }, -1, SRC)).toEqual({ i: 0, t: 59 / 60 })
    expect(pas(m, { i: 0, t: 0 }, -1, SRC)).toEqual({ i: 0, t: 0 })
    expect(pas(m, { i: 0, t: 599 / 60 }, 1, SRC)).toEqual({ i: 1, t: 0 })
    expect(pas(m, { i: 1, t: 0 }, -1, SRC)).toEqual({ i: 0, t: 599 / 60 })
    expect(pas(m, { i: 1, t: 1 }, 1, SRC)).toEqual({ i: 1, t: 31 / 30 })
    expect(pas(m, { i: 2, t: 1151 / 144 }, 1, SRC)).toEqual({ i: 2, t: 1151 / 144 })
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

describe('plusieurs clips', () => {
  it('ajoute des clips à la suite, avec et sans piste micro, volumes clip par clip', () => {
    const m = tous()
    expect(ordre(m)).toBe('J0 H0 C0')
    expect([clipsDe(m), dureeFinale(m), axe(m).debuts]).toEqual([['Jeu.mp4', 'Hevc.mp4', 'Carre.mp4'], 23, [0, 10, 18]])
    expect(m.volumes).toEqual({ 'Jeu.mp4': [{ volume: 1, muet: false }, { volume: 1, muet: false }], 'Hevc.mp4': [{ volume: 1, muet: false }], 'Carre.mp4': [] })
    expect(ajouterClip(m, H)).toBe(m)
    const regle = regler(regler(m, H, 0, { volume: 1.5 }), S, 1, { muet: true })
    expect([gain(regle, S, 0), gain(regle, S, 1), gain(regle, H, 0), regler(regle, C, 0, { volume: 0.5 })]).toEqual([1, 0, 1.5, regle])
    expect(sourcesDe([{ chemin: 'c', nom: 'Jeu', source: 'Jeu.mp4', dureeS: 10, largeur: 1920, hauteur: 1080, ips: 60, codec: 'h264', apercu: 'a', pistes: [{ nom: 'Jeu', chemin: 'p' }, { nom: 'Micro', chemin: 'q' }] }])).toEqual({ 'Jeu.mp4': S })
  })
  it('réordonne les segments et les clips, rogne et coupe dans le bon clip', () => {
    const m = couper(couper(tous(), S, 4, 0), H, 2, 2)
    expect(ordre(m)).toBe('J0 J4 H0 H2 C0')
    expect([bloc(m, 0), bloc(m, 1), bloc(m, 3), bloc(m, 4)]).toEqual([[0, 2], [0, 2], [2, 4], [4, 5]])
    const d = deplacer(m, 1, 3)
    expect(ordre(d)).toBe('J0 H0 H2 J4 C0')
    expect([bloc(d, 0), bloc(d, 3), bornes(d, 3), bornes(d, 1)]).toEqual([[0, 1], [3, 4], { debut: 4, fin: 10 }, { debut: 0, fin: 8 }])
    expect([deplacer(m, 1, 1), deplacer(m, -1, 2), deplacer(m, 0, 9)]).toEqual([m, m, m])
    expect(ordre(deplacer(d, 4, 0))).toBe('C0 J0 H0 H2 J4')
    expect(ordre(deplacerClip(d, 'Carre.mp4', 0))).toBe('C0 J0 J4 H0 H2')
    expect(ordre(deplacerClip(d, 'Jeu.mp4', 2))).toBe('H0 H2 C0 J0 J4')
    expect([deplacerClip(m, 'Jeu.mp4', 0), deplacerClip(m, 'Absent.mp4', 1), deplacerClip(m, 'Jeu.mp4', 5)]).toEqual([m, m, m])
    const r = rogner(rogner(d, S, 'debut', 5, 3), H, 'fin', 3, 1)
    expect(r.segments.map((s) => [s.source[0], s.debut, s.fin])).toEqual([['J', 0, 4], ['H', 0, 2], ['H', 2, 3], ['J', 5, 10], ['C', 0, 5]])
    expect(rogner(d, C, 'fin', 99, 4)).toBe(d)
    expect(ordre(couper(d, S, 7, 3))).toBe('J0 H0 H2 J4 J7 C0')
  })
  it('retire un clip du montage sans toucher aux autres, jamais le dernier', () => {
    const m = regler(couper(tous(), H, 2, 1), H, 0, { volume: 0.5 })
    const sans = retirerClip(m, 'Hevc.mp4')
    expect([ordre(sans), Object.keys(sans.volumes), dureeFinale(sans)]).toEqual(['J0 C0', ['Jeu.mp4', 'Carre.mp4'], 15])
    expect([retirerClip(m, 'Absent.mp4'), retirerClip(initial(S), 'Jeu.mp4')]).toEqual([m, initial(S)])
  })
  it('refuse au-delà de vingt clips ou de trente minutes', () => {
    const clip = (i: number, dureeS = 10): Source => ({ ...S, nom: `c${i}.mp4`, dureeS })
    const vingt = Array.from({ length: CLIPS_MAX - 1 }, (_, i) => clip(i)).reduce(ajouterClip, initial(S))
    expect([clipsDe(vingt).length, refus(initial(S), vingt)]).toEqual([20, null])
    expect(refus(vingt, ajouterClip(vingt, clip(99)))).toContain('20 clips')
    expect(refus(vingt, retirerClip(vingt, 'c3.mp4'))).toBeNull()
    const long = ajouterClip(initial(clip(0, 900)), clip(1, 900))
    expect([dureeFinale(long), refus(initial(S), long)]).toEqual([1800, null])
    expect(refus(long, ajouterClip(long, clip(2, 1)))).toContain('30 minutes')
    expect(refus(long, reglerSegment(long, 0, { vitesse: 0.5 }))).toContain('30 minutes')
    expect(refus(long, reglerSegment(long, 0, { vitesse: 2 }))).toBeNull()
    const trop = reglerSegment(long, 0, { vitesse: 0.25 })
    expect(refus(trop, reglerSegment(trop, 0, { vitesse: 0.5 }))).toBeNull()
  })
})

describe('vitesse', () => {
  it('change la durée finale et le temps du montage', () => {
    const m = [0.5, 2, 0.25, 4, 1.5].reduce((x, v, i) => reglerSegment(x, i, { vitesse: v }), coupes(rogner(initial(S), S, 'fin', 5), 1, 2, 3, 4))
    expect(m.segments.map((s) => s.vitesse)).toEqual([0.5, 2, 0.25, 4, 1.5])
    expect(dureeFinale(m)).toBeCloseTo(2 + 0.5 + 4 + 0.25 + 1 / 1.5, 9)
    expect([0.5, 1, 1.5, 2, 3, 5].map((u) => versMontage(m, u))).toEqual([1, 2, 2.25, 2.5, 6.5, 6.75 + 1 / 1.5])
    expect([1, 2, 2.25, 4.5, 6.5].map((t) => versAxe(m, t))).toEqual([0.5, 1, 1.5, 2.5, 3])
    expect(dureeFinale(retirer(m, 2))).toBeCloseTo(2 + 0.5 + 0.25 + 1 / 1.5, 9)
    expect([reglerSegment(m, 0, { vitesse: 0.5 }), reglerSegment(m, 0, { vitesse: 3 }), reglerSegment(m, 9, { vitesse: 2 })]).toEqual([m, m, m])
    expect(couper(m, S, 0.5).segments.slice(0, 2).map((s) => s.vitesse)).toEqual([0.5, 0.5])
  })
  it('coupe le son d’un segment et signale un ralenti saccadé', () => {
    const m = reglerSegment(initial(S), 0, { muet: true })
    expect([m.segments[0].muet, reglerSegment(m, 0, { muet: true }), reglerSegment(m, 0, { muet: false })]).toEqual([true, m, initial(S)])
    expect([saccade({ vitesse: 0.5 }, { ips: 60 }, 60), saccade({ vitesse: 0.5 }, { ips: 144 }, 60), saccade({ vitesse: 0.25 }, { ips: 144 }, 60), saccade({ vitesse: 1 }, { ips: 30 }, 60), saccade({ vitesse: 0.5 }, { ips: 60 }, 30), saccade({ vitesse: 2 }, { ips: 30 }, 60)]).toEqual([true, false, true, true, false, false])
  })
})

describe('géométrie du cadre et de la sortie', () => {
  const cadre = (x: number, y: number, l: number, h: number, rapport: Cadre['rapport'] = 'libre'): Cadre => ({ x, y, l, h, rapport })
  it('inscrit un clip dans la toile sans déformation, avec des bandes', () => {
    expect([toile(1920, 1080), toile(1921, 1081), toile(1, 1)]).toEqual([[1920, 1080], [1920, 1080], [2, 2]])
    expect(inscrire(1920, 1080, [1920, 1080])).toEqual([1920, 1080, 0, 0])
    expect(inscrire(2560, 1440, [1920, 1080])).toEqual([1920, 1080, 0, 0])
    expect(inscrire(1280, 720, [1920, 1080])).toEqual([1920, 1080, 0, 0])
    expect(inscrire(1000, 1000, [1920, 1080])).toEqual([1080, 1080, 420, 0])
    expect(inscrire(640, 360, [320, 240])).toEqual([320, 180, 0, 30])
    expect(inscrire(1080, 1920, [1920, 1080])).toEqual([608, 1080, 656, 0])
    expect(inscrire(2560, 1080, [1920, 1080])).toEqual([1920, 810, 0, 134])
  })
  it('convertit le cadre en pixels pairs dans la toile', () => {
    expect(cadrePixels(cadre(0, 0, 1, 1), [1920, 1080])).toEqual([1920, 1080, 0, 0])
    expect(cadrePixels(cadre(0.341796875, 0, 0.31640625, 1), [1920, 1080])).toEqual([608, 1080, 656, 0])
    expect(cadrePixels(cadre(0.25, 0.25, 0.5, 0.5), [1920, 1080])).toEqual([960, 540, 480, 270])
    expect(cadrePixels(cadre(0.9, 0.9, 0.5, 0.5), [1920, 1080])).toEqual([960, 540, 960, 540])
    expect(cadrePixels(cadre(0.1, 0.2, 0.333, 0.333), [1280, 720])).toEqual([426, 240, 128, 144])
    expect(cadrePixels(cadre(0, 0, 0.0001, 0.0001), [1280, 720])).toEqual([2, 2, 0, 0])
  })
  it('déduit la résolution de sortie du cadre, sans agrandissement', () => {
    expect(dimensionsSortie([1920, 1080], 'libre', null)).toEqual([1920, 1080])
    expect(dimensionsSortie([1920, 1080], 'libre', 720)).toEqual([1280, 720])
    expect(dimensionsSortie([1920, 1080], 'libre', 2160)).toEqual([1920, 1080])
    expect(dimensionsSortie([426, 240], 'libre', 121)).toEqual([216, 122])
    expect(dimensionsSortie([1920, 1080], '16:9', null)).toEqual([1920, 1080])
    expect(dimensionsSortie([1920, 1080], '16:9', 720)).toEqual([1280, 720])
    expect(dimensionsSortie([608, 1080], '9:16', null)).toEqual([594, 1056])
    expect(dimensionsSortie([608, 1080], '9:16', 720)).toEqual([396, 704])
    expect(dimensionsSortie([1080, 1080], '1:1', null)).toEqual([1080, 1080])
    expect(dimensionsSortie([864, 1080], '4:5', null)).toEqual([864, 1080])
    expect(dimensionsSortie([136, 240], '9:16', null)).toEqual([126, 224])
  })
  it('borne le cadre et tient les rapports proposés', () => {
    const t: [number, number] = [1920, 1080]
    expect(cadreAu('9:16', cadre(0, 0, 1, 1), t)).toEqual(cadre(0.341796875, 0, 0.31640625, 1, '9:16'))
    expect(cadreAu('16:9', cadre(0, 0, 1, 1), t)).toEqual(cadre(0, 0, 1, 1, '16:9'))
    expect(cadreAu('1:1', cadre(0, 0, 1, 1), t)).toEqual(cadre(0.21875, 0, 0.5625, 1, '1:1'))
    const portrait = cadreAu('4:5', cadre(0.5, 0, 0.5, 1), t)
    expect([portrait.l, portrait.x, portrait.h, portrait.y, portrait.rapport].map((x) => (typeof x === 'number' ? Math.round(x * 1e9) / 1e9 : x))).toEqual([0.45, 0.525, 1, 0, '4:5'])
    expect(cadreAu('libre', cadre(0.1, 0.1, 0.5, 0.5, '1:1'), t)).toEqual(cadre(0.1, 0.1, 0.5, 0.5))
    expect(cadreAu('16:9', cadre(0, 0, 1, 1), [1080, 1920])).toEqual(cadre(0, 0.341796875, 1, 0.31640625, '16:9'))
    expect(bornerCadre(cadre(0.9, -1, 0.5, 9), t)).toEqual(cadre(0.5, 0, 0.5, 1))
    expect(bornerCadre(cadre(0, 0, 0, Number.NaN), t)).toEqual(cadre(0, 0, 0.05, 1))
    const vertical = bornerCadre(cadre(0.9, 0.5, 0.2, 0.9, '9:16'), t)
    expect([vertical.l, vertical.h, vertical.x, vertical.y]).toEqual([0.2, 0.2 / 0.31640625, 0.8, 1 - 0.2 / 0.31640625])
    expect(vertical.l * 1920 / (vertical.h * 1080)).toBeCloseTo(9 / 16, 9)
    const m = initial(S)
    expect([recadrer(m, cadre(0, 0, 1, 1), t), recadrer(m, cadre(0.25, 0.25, 0.5, 0.5), t).cadre]).toEqual([m, cadre(0.25, 0.25, 0.5, 0.5)])
  })
  it('donne le format de sortie du montage et les réglages possibles', () => {
    const m = tous()
    expect(formatDe(m, SRC)).toEqual({ toile: [1920, 1080], cadre: [1920, 1080, 0, 0], largeur: 1920, hauteur: 1080, ips: 60 })
    const vertical = reglerSortie(recadrer(m, cadreAu('9:16', m.cadre, [1920, 1080]), [1920, 1080]), { hauteur: 720, ips: 144 })
    expect(formatDe(vertical, SRC)).toEqual({ toile: [1920, 1080], cadre: [608, 1080, 656, 0], largeur: 396, hauteur: 704, ips: 144 })
    expect(formatDe(retirerClip(vertical, 'Hevc.mp4'), SRC).ips).toBe(60)
    expect(formatDe(deplacerClip(m, 'Hevc.mp4', 0), SRC)).toEqual({ toile: [2560, 1440], cadre: [2560, 1440, 0, 0], largeur: 2560, hauteur: 1440, ips: 144 })
    expect([hauteursPossibles(formatDe(m, SRC)), hauteursPossibles(formatDe(deplacerClip(m, 'Hevc.mp4', 0), SRC)), cadencesPossibles(m, SRC), cadencesPossibles(initial(C), SRC)]).toEqual([[720, 480], [1080, 720, 480], [144, 120, 60, 30], [30]])
    expect([reglerSortie(m, { hauteur: null }), reglerSortie(m, { ips: 30 }).sortie]).toEqual([m, { hauteur: null, ips: 30 }])
  })
})

describe('texte', () => {
  const pinceau = () => {
    const appels: unknown[][] = []
    const c: Pinceau = {
      font: '', textAlign: '', textBaseline: '', lineJoin: '', lineWidth: 0, fillStyle: '', strokeStyle: '', globalAlpha: 1,
      clearRect: (...a) => appels.push(['efface', ...a]), fillRect: (...a) => appels.push(['fond', c.fillStyle, c.globalAlpha, ...a]),
      measureText: (texte) => ({ width: [...texte].length * Number(/(\d+)px/.exec(c.font)![1]) * 0.5 }),
      fillText: (...a) => appels.push(['texte', c.fillStyle, ...a]), strokeText: (...a) => appels.push(['contour', c.strokeStyle, c.lineWidth, ...a]),
    }
    return { c, appels }
  }
  const T: Texte = { contenu: 'Bien joué 🎉', police: 'Bricolage Grotesque', taille: 0.1, couleur: '#ffffff', habillage: 'contour', x: 0.5, y: 0.8, debut: 2, fin: 5 }
  it('pose des blocs sur un intervalle, plusieurs à la fois', () => {
    const m = ajouterTexte(ajouterTexte(initial(S), 2), 9.9)
    expect(m.textes.map((t) => [t.debut, t.fin, t.contenu, t.police, t.habillage])).toEqual([[2, 5, 'Texte', 'Bricolage Grotesque', 'contour'], [9.5, 10, 'Texte', 'Bricolage Grotesque', 'contour']])
    const r = reglerTexte(reglerTexte(m, 0, { debut: 2, fin: 5, contenu: 'Bien joué 🎉\r\n\u0007ligne 2' }), 1, { debut: 4, fin: 99, x: 3, y: -1, taille: 9, couleur: 'rouge', police: 'Comic Sans MS' })
    expect(r.textes[0].contenu).toBe('Bien joué 🎉\nligne 2')
    expect(r.textes[1]).toEqual({ ...m.textes[1], debut: 4, fin: 10, x: 1, y: 0, taille: 0.4 })
    expect([textesA(r, 1.99), textesA(r, 2), textesA(r, 4.5), textesA(r, 5), textesA(r, 10)]).toEqual([[], [0], [0, 1], [1], []])
    expect([reglerTexte(r, 0, { contenu: '  ' }), reglerTexte(r, 0, { x: 0.5 }), reglerTexte(r, 7, { x: 0.1 }), retirerTexte(r, 7)]).toEqual([r, r, r, r])
    expect(reglerTexte(r, 0, { contenu: 'é'.repeat(500) }).textes[0].contenu).toHaveLength(200)
    expect(reglerTexte(r, 0, { fin: 1 }).textes[0].fin).toBeCloseTo(2.1, 9)
    expect(retirerTexte(r, 0).textes).toEqual([r.textes[1]])
    expect(Array.from({ length: 60 }).reduce((x: Montage) => ajouterTexte(x, 0), initial(S)).textes).toHaveLength(50)
  })
  it('place le texte dans le cadre final, à la même position et à la même taille relatives quelle que soit la résolution', () => {
    const grand = pinceau()
    const petit = pinceau()
    const b = dessinerTexte(grand.c, T, 1920, 1080)
    const p = dessinerTexte(petit.c, T, 640, 360)
    for (const k of ['x', 'y', 'l', 'h'] as const) expect(p[k]).toBeCloseTo(b[k], 9)
    expect(grand.c.font).toBe(policeCss(T, 108))
    expect([corps(T, 1920, 1080), corps(T, 1080, 1920), corps(T, 594, 1056), corps({ taille: 0 }, 10, 10)]).toEqual([108, 108, 59, 1])
    expect(petit.c.font).toBe("700 36px 'Bricolage Grotesque Variable', 'Bricolage Grotesque', system-ui, sans-serif")
    expect(grand.appels).toEqual([['efface', 0, 0, 1920, 1080], ['contour', '#000000', 108 * 0.16, 'Bien joué 🎉', 960, 864], ['texte', '#ffffff', 'Bien joué 🎉', 960, 864]])
    expect(petit.appels.at(-1)).toEqual(['texte', '#ffffff', 'Bien joué 🎉', 320, 288])
    expect([b.x + b.l / 2, b.y + b.h / 2].map((x) => Math.round(x * 1e9) / 1e9)).toEqual([0.5, 0.8])
    expect(b.l).toBeCloseTo((11 * 54 + 21.6) / 1920, 9)
    const vertical = pinceau()
    const v = dessinerTexte(vertical.c, { ...T, x: 0.99, y: 1, habillage: 'fond', couleur: '#101010', contenu: 'Haut\nBas' }, 594, 1056)
    expect(v.x + v.l).toBeCloseTo(1, 9)
    expect(v.y + v.h).toBeCloseTo(1, 9)
    expect(v.x).toBeGreaterThanOrEqual(0)
    expect(vertical.appels.map((a) => a[0])).toEqual(['efface', 'fond', 'texte', 'texte'])
    expect(vertical.appels[1].slice(1, 3)).toEqual(['#ffffff', 0.72])
    expect((vertical.appels[3][4] as number) - (vertical.appels[2][4] as number)).toBeCloseTo(59 * 1.25, 9)
    const large = pinceau()
    const d = dessinerTexte(large.c, { ...T, contenu: 'x'.repeat(100), habillage: 'aucun', x: 0 }, 594, 1056)
    expect(large.appels.at(-1)!.slice(3)).toEqual([297, 0.8 * 1056])
    expect(d.l).toBeGreaterThan(1)
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
  it('couvre les opérations de l’éditeur avancé : clips, ordre, vitesse, son, cadre, texte, sortie', () => {
    const t: [number, number] = [1920, 1080]
    const operations: ((m: Montage) => Montage)[] = [
      (m) => ajouterClip(m, H), (m) => ajouterClip(m, C), (m) => couper(m, S, 4, 0), (m) => deplacer(m, 1, 3), (m) => deplacerClip(m, 'Carre.mp4', 0), (m) => reglerSegment(m, 1, { vitesse: 0.5 }),
      (m) => reglerSegment(m, 2, { muet: true }), (m) => regler(m, H, 0, { volume: 1.5 }), (m) => recadrer(m, cadreAu('9:16', m.cadre, t), t), (m) => recadrer(m, { ...m.cadre, x: 0.1 }, t),
      (m) => ajouterTexte(m, 2), (m) => ajouterTexte(m, 3), (m) => reglerTexte(m, 0, { contenu: 'GG', x: 0.2, y: 0.3 }), (m) => reglerTexte(m, 1, { police: 'JetBrains Mono', taille: 0.12, couleur: '#ff0000', habillage: 'fond', debut: 1, fin: 4 }),
      (m) => retirerTexte(m, 0), (m) => reglerSortie(m, { hauteur: 720, ips: 30 }), (m) => rogner(m, H, 'fin', 3, 2), (m) => retirerClip(m, 'Jeu.mp4'),
    ]
    const etats = [initial(S)]
    let h = historique(etats[0])
    for (const op of operations) {
      h = appliquer(h, op(h.present))
      etats.push(structuredClone(h.present))
    }
    expect(new Set(etats.map((e) => JSON.stringify(e))).size).toBe(operations.length + 1)
    expect([ordre(h.present), h.present.textes.length, h.present.cadre.rapport, h.present.sortie]).toEqual(['C0 H0', 1, '9:16', { hauteur: 720, ips: 30 }])
    for (let i = operations.length - 1; i >= 0; i--) {
      h = annuler(h)
      expect(h.present).toEqual(etats[i])
    }
    for (let i = 1; i <= operations.length; i++) {
      h = retablir(h)
      expect(h.present).toEqual(etats[i])
    }
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
  const complet = () => {
    const t: [number, number] = [1920, 1080]
    const m = reglerSegment(deplacer(couper(regler(tous(), S, 0, { volume: 0.5 }), S, 4, 0), 1, 3), 1, { vitesse: 0.25 })
    return reglerSortie(reglerTexte(ajouterTexte(recadrer(m, cadreAu('9:16', m.cadre, t), t), 2), 0, { contenu: 'Bien joué 🎉', habillage: 'fond' }), { hauteur: 720, ips: 30 })
  }
  it('relit un projet tel qu’il a été écrit', () => {
    const m = regler(trois(), S, 0, { volume: 0.5 })
    expect(lireProjet(JSON.parse(JSON.stringify(m)), [S], 'x')).toEqual(m)
    expect(lireProjet(JSON.parse(JSON.stringify(complet())), [S, H, C], 'x')).toEqual(complet())
  })
  it('signale un clip manquant par son absence et ouvre le reste', () => {
    const m = lireProjet(JSON.parse(JSON.stringify(complet())), [S, C], 'x')
    expect([ordre(m), Object.keys(m.volumes), m.textes.length, m.cadre.rapport, m.sortie]).toEqual(['J0 C0 J4', ['Jeu.mp4', 'Carre.mp4'], 1, '9:16', { hauteur: 720, ips: 30 }])
    expect(clipsDe(complet()).filter((c) => !clipsDe(m).includes(c))).toEqual(['Hevc.mp4'])
    expect(lireProjet(JSON.parse(JSON.stringify(complet())), [C], 'x').segments).toEqual(initial(C).segments)
    expect(lireProjet(JSON.parse(JSON.stringify(initial(H))), [S], 'Secours')).toEqual(initial(S, 'Secours'))
  })
  it('ramène les temps sur les images et les réglages dans leurs bornes', () => {
    const m = lireProjet({ version: 2, nom: ' ', segments: [{ source: 'Jeu.mp4', debut: 0.504, fin: 2, vitesse: 3 }, { source: 'Jeu.mp4', debut: 2, fin: 99, garde: false, vitesse: 0.5, muet: true }], volumes: { 'Jeu.mp4': [{ volume: 5, muet: true }] }, textes: [null, { contenu: 'a', x: 7 }, { contenu: '' }], cadre: { x: 0.9, l: 0.5, rapport: 'faux' }, sortie: { hauteur: 99999, ips: 0 } }, [S], 'Jeu')
    expect(m.segments).toEqual([{ source: 'Jeu.mp4', debut: 0.5, fin: 2, garde: true, vitesse: 1, muet: false }, { source: 'Jeu.mp4', debut: 2, fin: 10, garde: false, vitesse: 0.5, muet: true }])
    expect([m.nom, m.volumes, m.cadre, m.sortie]).toEqual(['Jeu', { 'Jeu.mp4': [{ volume: 2, muet: true }, { volume: 1, muet: false }] }, { x: 0.5, y: 0, l: 0.5, h: 1, rapport: 'libre' }, { hauteur: 4320, ips: null }])
    expect(m.textes).toEqual([{ contenu: 'a', police: 'Bricolage Grotesque', taille: 0.08, couleur: '#ffffff', habillage: 'contour', x: 1, y: 0.5, debut: 0, fin: 3 }])
  })
  it('repart du clip entier si le projet est illisible ou d’une autre version', () => {
    const seg = (debut: number, fin: number) => ({ source: 'Jeu.mp4', debut, fin, garde: true })
    for (const brut of [null, 'x', 3, {}, { version: 1, segments: [seg(0, 1)] }, { version: 3, segments: [seg(0, 1)] }, { version: 2, segments: [] }, { version: 2, segments: [seg(2, 1)] }, { version: 2, segments: [null] }, { version: 2, segments: [{ source: 'Jeu.mp4', debut: '0', fin: 1 }] }, { version: 2, segments: [seg(20, 30)] }]) {
      expect(lireProjet(brut, [S], 'Jeu')).toEqual(initial(S))
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
