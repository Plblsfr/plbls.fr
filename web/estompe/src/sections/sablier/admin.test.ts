import { describe, expect, it } from 'vitest'
import { avecCompte, choisirExploit, choisirStat, cleDepuisNom, depuisSaisie, enSaisie, filtrerJournal, filtrerPersonnes, formulaireDe, formulaireVide, ligneTrophee, nomCible, nomPersonne, ongletDe, resumeTrophees, sansCompte, versRequete, type Formulaire } from './admin'
import { catalogue, journal, personnes, tropheesAdmin } from './essais'

const ICONES = ['trophee', 'film', 'casque', 'mic']
const c = catalogue()

describe('conversions d’unités des seuils', () => {
  it('saisit les durées en heures et les envoie en secondes', () => {
    expect(enSaisie(86_400, 'seconds')).toBe('24')
    expect(enSaisie(5400, 'seconds')).toBe('1,5')
    expect(depuisSaisie('24', 'seconds')).toBe(86_400)
    expect(depuisSaisie('1,5', 'seconds')).toBe(5400)
    expect(depuisSaisie(' 0.25 ', 'seconds')).toBe(900)
    expect(enSaisie(50, 'count')).toBe('50')
    expect(depuisSaisie('1 000', 'count')).toBe(1000)
    expect(depuisSaisie('1,5', 'count')).toBeNull()
    expect(depuisSaisie('', 'count')).toBeNull()
    expect(depuisSaisie('-3', 'days')).toBeNull()
    expect(depuisSaisie('abc', 'seconds')).toBeNull()
  })
})

describe('formulaire de trophée', () => {
  const tiers = (over: Partial<Formulaire> = {}): Formulaire => ({ ...formulaireVide(c), cle: 'melomane', nom: 'Mélomane', description: 'Du temps passé à écouter.', icone: 'casque', stat: 'sillon.seconds', periode: 'all', seuils: ['10', '100'], ...over })

  it('C7 : crée un trophée à paliers avec la requête attendue par le serveur', () => {
    expect(versRequete(tiers(), c, ICONES)).toEqual({ ok: true, corps: { key: 'melomane', name: 'Mélomane', description: 'Du temps passé à écouter.', icon: 'casque', hidden: false, active: true, kind: 'tiers', stat: 'sillon.seconds', period: 'all', thresholds: [36_000, 360_000] } })
  })

  it('C7 : crée un exploit codé avec son paramètre, sans champs des paliers ; sans paramètre s’il n’en prend pas', () => {
    const f = choisirExploit(tiers({ genre: 'exploit' }), c, 'long_appel')
    expect(f.parametre).toBe('3')
    expect(versRequete({ ...f, parametre: '2' }, c, ICONES)).toEqual({ ok: true, corps: { key: 'melomane', name: 'Mélomane', description: 'Du temps passé à écouter.', icon: 'casque', hidden: false, active: true, kind: 'exploit', exploit: 'long_appel', parameter: 7200 } })
    const sans = versRequete(choisirExploit(f, c, 'mois_parfait'), c, ICONES)
    expect(sans.ok && 'parameter' in sans.corps).toBe(false)
  })

  it('refuse comme le serveur : clé, longueurs, icône, seuils croissants et positifs, six paliers au plus', () => {
    const erreurs = (f: Formulaire) => { const r = versRequete(f, c, ICONES); return r.ok ? {} : r.erreurs }
    expect(Object.keys(erreurs(tiers({ cle: 'Mélo mane', nom: ' ', description: 'x'.repeat(201), icone: 'inconnue' }))).sort()).toEqual(['cle', 'description', 'icone', 'nom'])
    expect(erreurs(tiers({ seuils: ['10', '10'] })).seuils).toBe('Les paliers doivent être strictement croissants.')
    expect(erreurs(tiers({ seuils: ['0'] })).seuils).toBe('Chaque palier est un nombre d’heures supérieur à zéro.')
    expect(erreurs(tiers({ stat: 'messaging.messages', seuils: ['1,5'] })).seuils).toBe('Chaque palier est un nombre entier supérieur à zéro.')
    expect(erreurs(tiers({ seuils: ['1', '2', '3', '4', '5', '6', '7'] })).seuils).toBe('De 1 à 6 paliers.')
    expect(erreurs(tiers({ stat: 'usage.first_day' })).stat).toBeDefined()
    expect(erreurs(tiers({ genre: 'exploit', exploit: 'long_appel', parametre: '' })).parametre).toBeDefined()
    expect(erreurs(tiers({ genre: 'exploit', exploit: 'inconnu' })).exploit).toBeDefined()
  })

  it('modifie en n’envoyant que ce qui change, jamais la clé ni le genre', () => {
    const platine = tropheesAdmin().find((t) => t.key === 'platine')!
    const f = formulaireDe(platine, c)
    expect(f.seuils).toEqual(['24', '100', '500', '2000'])
    expect(versRequete(f, c, ICONES, platine)).toEqual({ ok: true, corps: {} })
    expect(versRequete({ ...f, nom: 'Platine ', seuils: ['24', '90', '500', '2000'], actif: false }, c, ICONES, platine)).toEqual({ ok: true, corps: { active: false, thresholds: [86_400, 324_000, 1_800_000, 7_200_000] } })
    const appel = tropheesAdmin().find((t) => t.key === 'appel_fleuve')!
    expect(versRequete({ ...formulaireDe(appel, c), parametre: '4' }, c, ICONES, appel)).toEqual({ ok: true, corps: { parameter: 14_400 } })
  })

  it('changer de stat garde les seuils si l’unité ne change pas, et fige la période d’une série', () => {
    expect(choisirStat(tiers(), c, 'usage.active_seconds').seuils).toEqual(['10', '100'])
    expect(choisirStat(tiers({ periode: 'month' }), c, 'streak.global')).toMatchObject({ seuils: [''], periode: 'all' })
  })

  it('propose une clé à partir du nom', () => {
    expect(cleDepuisNom('  Mélomane averti ! ')).toBe('melomane_averti')
  })
})

describe('listes de l’administration', () => {
  it('C6 : sépare les personnes sans compte et les nomme', () => {
    const p = personnes().people
    expect(avecCompte(p).map((x) => x.sub)).toEqual(['kc-camille', 'kc-moi'])
    expect(sansCompte(p).map(nomPersonne)).toEqual(['Compte supprimé · 7f3a…c2'])
    expect(nomPersonne({ sub: 'kc-x', name: null, known: true })).toBe('Sans nom · kc-x')
  })

  it('cherche par nom sans tenir compte des accents ni de la casse, ou par identifiant', () => {
    const p = [...personnes().people, { ...personnes().people[0], sub: 'kc-noe', name: 'Noé' }]
    expect(filtrerPersonnes(p, 'NOE').map((x) => x.sub)).toEqual(['kc-noe'])
    expect(filtrerPersonnes(p, '7f3a').map((x) => x.known)).toEqual([false])
    expect(filtrerPersonnes(p, '  ')).toHaveLength(p.length)
    expect(filtrerPersonnes(p, 'zzz')).toEqual([])
  })

  it('filtre et nomme les lignes du journal', () => {
    const j = journal().entries
    expect(filtrerJournal(j, 'camille').map((e) => e.id)).toEqual([3, 1])
    expect(filtrerJournal(j, 'ines').map((e) => e.id)).toEqual([2])
    expect(nomCible(j[1])).toBe('Sans nom · 7f3a…c2')
  })

  it('décrit une ligne de trophée : type, stat, période et seuils dans leur unité', () => {
    const liste = tropheesAdmin()
    expect(ligneTrophee(liste[0], c)).toEqual({ type: 'Paliers', code: false, stat: 'Temps d’écoute', periode: 'Tout', seuils: '24 h · 100 h · 500 h · 2 000 h' })
    expect(ligneTrophee(liste[1], c)).toEqual({ type: 'Exploit', code: true, stat: 'Long appel', periode: '—', seuils: '3 h' })
    expect(ligneTrophee(liste[2], c)).toMatchObject({ type: 'Série de jours', periode: '—', seuils: '3 · 7' })
    expect(resumeTrophees(liste)).toBe('3 trophées, 2 actifs.')
  })

  it('retombe sur l’onglet Personnes pour un onglet inconnu', () => {
    expect(ongletDe('journal')).toBe('journal')
    expect(ongletDe('ailleurs')).toBe('personnes')
    expect(ongletDe(undefined)).toBe('personnes')
  })
})
