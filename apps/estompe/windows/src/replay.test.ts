import { describe, expect, it } from 'vitest'
import { overlayAccelerator } from './overlay'
import {
  ajouterJeu, ajouterMessage, coinOverlay, dateClip, decisionEnvoi, detailsClip, DEFAUTS, dureeClip, dureeTampon, ecrireReglages, ETAT_INITIAL, etatOverlay, GO, jeuxDe, libelleMicro, lireReglages,
  messageClip, messageErreur, messageEtat, nomExecutable, partUtilisee, raccourciLisible, reglagesMoteur, taille, tailleOverlay, TAILLE_OVERLAY, trierClips, type Clip, type Etat, type MessageOverlay,
} from './replay'

const MO = 1024 ** 2
const clip = (p: Partial<Clip>): Clip => ({ chemin: `C:\\Clips\\${p.nom ?? 'a'}.mp4`, nom: 'a', jeu: 'Jeu', date: '2026-11-12T10:00:00', dureeS: 30, tailleOctets: 40 * MO, vignette: null, codec: 'h264', largeur: 1920, hauteur: 1080, ips: 60, pistesAudio: 1, ...p })
const etat = (p: Partial<Etat>): Etat => ({ ...ETAT_INITIAL, ...p })

describe('réglages de l’instant replay', () => {
  it('part des défauts de l’issue, fonction désactivée', () => {
    expect(lireReglages(null, '1.0.0')).toEqual(DEFAUTS)
    expect(DEFAUTS).toMatchObject({ actif: false, resolution: '1080p', ips: 60, dureeS: 30, qualite: 'haute', micro: false, curseur: false, raccourci: 'Alt+F10', dossier: null, quotaGo: 20, jeuxAjoutes: [], jeuxExclus: [] })
  })
  it('garde les réglages et l’activation tant que la version de l’app ne change pas', () => {
    const r = { ...DEFAUTS, actif: true, resolution: '1440p' as const, ips: 144, dureeS: 120, micro: true, jeuxExclus: ['a.exe'] }
    expect(lireReglages(ecrireReglages(r, '1.2.0'), '1.2.0')).toEqual(r)
  })
  it('désactive la fonction après une mise à jour, sans toucher aux autres réglages', () => {
    const r = { ...DEFAUTS, actif: true, ips: 120, raccourci: 'Control+F9', dossier: 'D:\\Clips', quotaGo: 50 }
    expect(lireReglages(ecrireReglages(r, '1.2.0'), '1.3.0')).toEqual({ ...r, actif: false })
    expect(lireReglages(JSON.stringify({ ...r }), '1.3.0').actif).toBe(false)
  })
  it('ramène une valeur illisible ou hors bornes à une valeur admise', () => {
    expect(lireReglages('{', '1')).toEqual(DEFAUTS)
    expect(lireReglages('null', '1')).toEqual(DEFAUTS)
    const r = lireReglages(JSON.stringify({ resolution: '8k', ips: 75, dureeS: 900, qualite: 'ultra', quotaGo: 0, raccourci: '', dossier: ' ', jeuxAjoutes: 'x', jeuxExclus: ['a.exe', 3, ' a.exe ', ''] }), '1')
    expect(r).toEqual({ ...DEFAUTS, dureeS: 300, quotaGo: 1, jeuxExclus: ['a.exe'] })
    expect(lireReglages(JSON.stringify({ dureeS: 2 }), '1').dureeS).toBe(15)
  })
  it('construit les réglages du moteur avec le micro des paramètres vocaux', () => {
    const r = { ...DEFAUTS, actif: true, micro: true, quotaGo: 5, jeuxAjoutes: ['C:\\Jeux\\a.exe'] }
    expect(reglagesMoteur(r, { libelle: 'Micro USB', reduction: 'standard', gain: 1.5 })).toEqual({
      actif: true, resolution: '1080p', ips: 60, dureeS: 30, qualite: 'haute', micro: true, microLibelle: 'Micro USB', microReduction: 'standard', microGain: 1.5,
      curseur: false, dossier: null, quotaOctets: 5 * GO, jeuxAjoutes: ['C:\\Jeux\\a.exe'], jeuxExclus: [],
    })
    expect(reglagesMoteur(r, { libelle: null, reduction: 'off', gain: 4 }).microGain).toBe(3)
    expect('raccourci' in reglagesMoteur(r, { libelle: null, reduction: 'off', gain: 1 })).toBe(false)
  })
  it('n’envoie pas les réglages vocaux tant que le micro n’est pas enregistré', () => {
    const a = reglagesMoteur(DEFAUTS, { libelle: 'Micro USB', reduction: 'off', gain: 2 })
    expect(a).toMatchObject({ micro: false, microLibelle: null, microReduction: 'strong', microGain: 1 })
    expect(a).toEqual(reglagesMoteur(DEFAUTS, { libelle: null, reduction: 'standard', gain: 0.5 }))
  })
  it('retrouve le libellé du micro choisi, ou rien pour le micro par défaut', () => {
    const l = [{ kind: 'audioinput' as const, deviceId: 'abc', label: ' Microphone (USB) (046d:0825) ' }, { kind: 'audiooutput' as const, deviceId: 'out', label: 'Casque' }, { kind: 'audioinput' as const, deviceId: 'vide', label: '' }]
    expect(libelleMicro(l, 'abc')).toBe('Microphone (USB) (046d:0825)')
    for (const id of ['', 'default', 'out', 'vide', 'inconnu']) expect(libelleMicro(l, id)).toBeNull()
  })
  it('tient les listes de jeux sans doublon et réduit un chemin au nom de l’exécutable', () => {
    expect(ajouterJeu(['C:\\a.exe'], ' c:\\A.exe ')).toEqual(['C:\\a.exe'])
    expect(ajouterJeu(['a.exe'], 'b.exe')).toEqual(['a.exe', 'b.exe'])
    expect(ajouterJeu(['a.exe'], '  ')).toEqual(['a.exe'])
    expect(nomExecutable('C:\\Jeux\\Mon Jeu\\jeu.exe')).toBe('jeu.exe')
    expect(nomExecutable('jeu.exe')).toBe('jeu.exe')
  })
})

describe('raccourci de sauvegarde', () => {
  const touche = (p: Partial<KeyboardEvent>) => overlayAccelerator({ code: 'F10', ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...p })
  it('le raccourci par défaut est celui que produit Alt+F10', () => {
    expect(touche({ altKey: true })).toBe(DEFAUTS.raccourci)
    expect(touche({ code: 'AltLeft', altKey: true })).toBe('')
  })
  it('s’affiche sans les préfixes des codes de touche', () => {
    expect(raccourciLisible('Alt+F10')).toBe('Alt + F10')
    expect(raccourciLisible('Control+Shift+KeyR')).toBe('Ctrl + Shift + R')
    expect(raccourciLisible('Digit5')).toBe('5')
  })
})

describe('formatage', () => {
  it('durée du tampon et durée d’un clip', () => {
    expect([15, 45, 60, 90, 300].map(dureeTampon)).toEqual(['15 s', '45 s', '1 min', '1 min 30', '5 min'])
    expect([0, 29.6, 65, 300].map(dureeClip)).toEqual(['0:00', '0:30', '1:05', '5:00'])
  })
  it('taille en Ko, Mo ou Go', () => {
    expect([0, 500, 300 * 1024, 850 * MO, 1.25 * GO, 20 * GO].map(taille)).toEqual(['0 Ko', '1 Ko', '300 Ko', '850 Mo', '1,3 Go', '20 Go'])
    expect(taille(2 * GO)).toBe('2 Go')
  })
  it('part du quota utilisée, plafonnée à 100', () => {
    expect(partUtilisee({ utiliseOctets: 5 * GO, quotaOctets: 20 * GO })).toBe(25)
    expect(partUtilisee({ utiliseOctets: 30 * GO, quotaOctets: 20 * GO })).toBe(100)
    expect(partUtilisee({ utiliseOctets: 1, quotaOctets: 0 })).toBe(0)
  })
  it('détails d’un clip, sans les informations absentes', () => {
    expect(detailsClip(clip({ codec: 'hevc', hauteur: 2160, ips: 144, pistesAudio: 2 }))).toBe('40 Mo · 2160p · 144 i/s · HEVC · piste micro')
    expect(detailsClip(clip({ codec: null, hauteur: 0, ips: 0, pistesAudio: 0 }))).toBe('40 Mo')
  })
  it('date d’un clip, vide si elle est illisible', () => {
    expect(dateClip('2026-11-12T10:05:00')).toContain('12 novembre 2026')
    expect(dateClip('')).toBe('')
  })
})

describe('galerie', () => {
  const clips = [clip({ nom: 'vieux', date: '2026-11-10T08:00:00' }), clip({ nom: 'recent', date: '2026-11-12T09:00:00', jeu: 'Autre jeu' }), clip({ nom: 'depose', date: '2026-11-11T08:00:00Z', jeu: null }), clip({ nom: 'milieu', date: '2026-11-11T20:00:00' })]
  it('trie du plus récent au plus ancien, dates locales et UTC mêlées', () => {
    expect(trierClips(clips).map((c) => c.nom)).toEqual(['recent', 'milieu', 'depose', 'vieux'])
    expect(clips[0].nom).toBe('vieux')
  })
  it('filtre par jeu ; un fichier sans annexe va dans « Autres »', () => {
    expect(trierClips(clips, 'Jeu').map((c) => c.nom)).toEqual(['milieu', 'vieux'])
    expect(trierClips(clips, 'Autres').map((c) => c.nom)).toEqual(['depose'])
    expect(jeuxDe(clips)).toEqual([{ jeu: 'Autre jeu', nombre: 1 }, { jeu: 'Jeu', nombre: 2 }, { jeu: 'Autres', nombre: 1 }])
  })
})

describe('envoi dans un salon', () => {
  const limite = 100 * MO
  it('part tel quel en H.264 à une piste sous la limite', () => {
    expect(decisionEnvoi(clip({}), limite)).toBe('direct')
  })
  it('passe par la conversion pour le HEVC, deux pistes audio ou un codec inconnu', () => {
    expect(decisionEnvoi(clip({ codec: 'hevc' }), limite)).toBe('h264')
    expect(decisionEnvoi(clip({ pistesAudio: 2 }), limite)).toBe('h264')
    expect(decisionEnvoi(clip({ codec: null, dureeS: 0 }), limite)).toBe('h264')
  })
  it('propose le réencodage plus léger au-delà de la limite', () => {
    expect(decisionEnvoi(clip({ tailleOctets: 400 * MO, dureeS: 300 }), limite)).toBe('leger')
    expect(decisionEnvoi(clip({ tailleOctets: 400 * MO, dureeS: 0, codec: null }), limite)).toBe('leger')
  })
  it('refuse quand le débit qui tiendrait dans la limite est illisible', () => {
    expect(decisionEnvoi(clip({ tailleOctets: 400 * MO, dureeS: 300 }), 15 * MO)).toBe('refus')
    expect(decisionEnvoi(clip({ tailleOctets: 400 * MO, dureeS: 300 }), 18 * MO)).toBe('leger')
  })
})

describe('overlay de l’instant replay', () => {
  const m = (id: string): MessageOverlay => ({ id, genre: 'info', texte: id })
  it('annonce la pause, l’indisponibilité et la reprise avec leur raison', () => {
    const arme = etat({ etat: 'arme', jeu: 'Jeu' })
    expect(messageEtat(arme, etat({ etat: 'pause', raison: 'le partage d\'écran est prioritaire' }))).toEqual({ genre: 'info', texte: 'Instant replay en pause : le partage d\'écran est prioritaire' })
    expect(messageEtat(arme, etat({ etat: 'indisponible', raison: 'capture refusée' }))).toEqual({ genre: 'erreur', texte: 'Instant replay indisponible : capture refusée' })
    expect(messageEtat(etat({ etat: 'pause' }), arme)).toEqual({ genre: 'info', texte: 'Instant replay repris' })
    expect(messageEtat(etat({ etat: 'pause', raison: 'a' }), etat({ etat: 'pause', raison: 'b' }))?.texte).toBe('Instant replay en pause : b')
  })
  it('ne dit rien pour un armement, une fermeture du jeu ou un état répété', () => {
    expect(messageEtat(ETAT_INITIAL, etat({ etat: 'arme' }))).toBeNull()
    expect(messageEtat(etat({ etat: 'arme' }), ETAT_INITIAL)).toBeNull()
    expect(messageEtat(etat({ etat: 'pause', raison: 'a' }), etat({ etat: 'pause', raison: 'a' }))).toBeNull()
  })
  it('signale le repli sur le micro par défaut une seule fois', () => {
    const repli = etat({ etat: 'arme', microParDefaut: true })
    expect(messageEtat(ETAT_INITIAL, repli)?.texte).toContain('micro par défaut')
    expect(messageEtat(repli, repli)).toBeNull()
  })
  it('confirme le clip et reprend le message d’une erreur', () => {
    expect(messageClip({ dureeS: 30 })).toEqual({ genre: 'ok', texte: 'Clip enregistré (0:30)' })
    expect(messageErreur({ code: 'disque-plein', message: 'Disque plein.' })).toEqual({ genre: 'erreur', texte: 'Disque plein.' })
    expect(messageErreur({ code: 'gpu', message: '' }).texte).toContain('gpu')
  })
  it('garde les derniers messages seulement', () => {
    expect(ajouterMessage([m('a'), m('b'), m('c')], m('d')).map((x) => x.id)).toEqual(['b', 'c', 'd'])
  })
  it('n’ouvre la fenêtre que pour l’indicateur ou un message', () => {
    expect(etatOverlay(ETAT_INITIAL, true, [])).toBeNull()
    expect(etatOverlay(etat({ etat: 'arme' }), false, [])).toBeNull()
    expect(etatOverlay(etat({ etat: 'arme' }), true, [])).toEqual({ indicateur: true, messages: [] })
    expect(etatOverlay(etat({ etat: 'pause' }), true, [m('a')])).toEqual({ indicateur: false, messages: [m('a')] })
  })
  it('dimensionne la fenêtre et évite le coin des participants', () => {
    const { l, indicateur, ligne, ecart } = TAILLE_OVERLAY
    expect(tailleOverlay({ indicateur: true, messages: [] })).toEqual({ w: l, h: indicateur })
    expect(tailleOverlay({ indicateur: true, messages: [m('a'), m('b')] })).toEqual({ w: l, h: indicateur + 2 * ligne + 2 * ecart })
    expect(tailleOverlay({ indicateur: false, messages: [m('a')] })).toEqual({ w: l, h: ligne })
    expect(coinOverlay('ml')).toBe('tr')
    expect(coinOverlay('tr')).toBe('tl')
  })
})
