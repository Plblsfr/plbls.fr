import { describe, expect, it } from 'vitest'
import { AUJOURDHUI, entracte, messagerie, series, sillon, statsPleines, statsVides, trophees, usage } from './essais'
import { aucuneActivite, barresParJour, carteEntracte, carteMessagerie, carteSillon, carteUsage, cartes, cartesSeries, estVide, SALON_SUPPRIME, vueTrophees, type NomSalon } from './stats'

const nom: NomSalon = (_s, salon) => ({ c1: 'général', v1: 'Canapé' })[salon] ?? null
const labels = (c: { lignes: { label: string }[] }) => c.lignes.map((l) => l.label)

describe('choix des stats à afficher', () => {
  it('C2 : une stat nulle sur la période n’a pas de ligne', () => {
    const s = statsPleines()
    const pleine = carteEntracte(s.entracte)
    expect(pleine.etat).toBe('pleine')
    expect(labels(pleine)).toEqual(['Plus longue session', 'Plus grosse journée', 'Épisodes enchaînés', 'Salon cinéma', 'Titres abandonnés avant la fin'])
    expect(labels(pleine)).not.toContain('Intros passées')
    const maigre = carteEntracte(entracte({ seconds: 3600, movies_watched: 1 }))
    expect(maigre.tuiles).toEqual([{ label: 'Film vu', valeur: '1' }])
    expect(maigre.lignes).toEqual([])
    expect(maigre.chiffre).toEqual({ valeur: '1 h', unite: 'regardées' })
  })

  it('C2 : un domaine sans accès donne la carte courte, un domaine sans donnée une carte vide', () => {
    expect(carteEntracte({ access: false })).toMatchObject({ etat: 'sans-acces', tuiles: [], lignes: [] })
    expect(carteSillon({ access: false }).etat).toBe('sans-acces')
    expect(carteEntracte(entracte()).etat).toBe('vide')
    expect(carteSillon(sillon()).etat).toBe('vide')
    expect(carteMessagerie(messagerie(), nom).etat).toBe('vide')
    expect(carteUsage(usage(), AUJOURDHUI).etat).toBe('vide')
    const ordre = cartes({ ...statsPleines(), entracte: entracte(), sillon: { access: false } }, nom, AUJOURDHUI).map((c) => `${c.domaine}:${c.etat}`)
    expect(ordre).toEqual(['usage:pleine', 'messaging:pleine', 'entracte:vide', 'sillon:sans-acces'])
    expect(estVide(statsVides(), 'messaging')).toBe(true)
    expect(estVide(statsPleines(), 'messaging')).toBe(false)
  })

  it('résout le nom des salons au moment de l’affichage, « salon supprimé » sinon, compteurs conservés', () => {
    const c = carteMessagerie(statsPleines().messaging, nom)
    expect(c.parts.map((p) => [p.label, p.valeur])).toEqual([['# général', '281'], [SALON_SUPPRIME, '139'], ['Privés', '118']])
    expect(c.lignes.find((l) => l.label === 'En vocal')?.valeur).toBe('9 h 20 · salon « Canapé »')
    const inconnu = carteMessagerie(statsPleines().messaging, () => null)
    expect(inconnu.parts[0]).toMatchObject({ label: SALON_SUPPRIME, valeur: '281' })
    expect(inconnu.lignes.find((l) => l.label === 'En vocal')?.valeur).toBe(`9 h 20 · ${SALON_SUPPRIME}`)
  })

  it('les conversations privées ne donnent qu’un total', () => {
    const c = carteMessagerie(messagerie({ total: 50, private: 50 }), nom)
    expect(c.chiffre?.valeur).toBe('50')
    expect(c.parts).toEqual([])
    expect(c.lignes).toEqual([])
  })

  it('regroupe les salons au-delà des trois premiers', () => {
    const salons = [5, 4, 3, 2, 1].map((n, i) => ({ server: 's1', channel: i === 0 ? 'c1' : `x${i}`, messages: n * 10 }))
    const c = carteMessagerie(messagerie({ total: 150, by_channel: salons }), nom)
    expect(c.parts.map((p) => [p.label, p.valeur])).toEqual([['# général', '50'], [SALON_SUPPRIME, '40'], [SALON_SUPPRIME, '30'], ['Autres salons', '30']])
  })

  it('carte d’utilisation : jours actifs sur jours écoulés, parts par section et plateforme seulement s’il y en a plusieurs', () => {
    const c = carteUsage(statsPleines().usage, AUJOURDHUI)
    expect(c.chiffre).toEqual({ valeur: '31 h 20', unite: 'de temps actif' })
    expect(c.lignes[0]).toEqual({ label: 'Jours actifs', valeur: '16 sur 18' })
    expect(c.parts.map((p) => p.label)).toEqual(['Entracte', 'Messagerie'])
    expect(c.etiquettes).toEqual(['Windows 89 %', 'Android 11 %'])
    expect(c.barres?.liste).toHaveLength(18)
    const seule = carteUsage(usage({ seconds: 600, active_days: 1, by_day: [{ day: '2026-11-18', seconds: 600 }], longest_day: { day: '2026-11-18', seconds: 600 }, by_section: [{ section: 'messagerie', seconds: 600 }], by_platform: [{ platform: 'web', seconds: 600 }] }), AUJOURDHUI)
    expect(seule.parts).toEqual([])
    expect(seule.etiquettes).toEqual([])
    expect(labels(seule)).toEqual(['Jours actifs'])
    expect(barresParJour(usage({ period: 'year', from: '2026-01-01', to: '2026-12-31', seconds: 10 }), AUJOURDHUI)).toEqual([])
    const tout = carteUsage(usage({ period: 'all', from: '2026-09-03', to: AUJOURDHUI, seconds: 7200, active_days: 3 }), AUJOURDHUI)
    expect(tout.lignes).toContainEqual({ label: 'Premier jour d’activité', valeur: '3 septembre 2026' })
  })

  it('séries : sans accès absentes, arrêtées en carte courte, jamais commencées absentes', () => {
    const liste = cartesSeries(series())
    expect(liste.map((s) => `${s.cle}:${s.etat}`)).toEqual(['global:en-cours', 'messaging:en-cours', 'entracte:arretee'])
    expect(liste[0]).toMatchObject({ nombre: '12', unite: 'jours d’affilée', record: 'Record : 41 jours' })
    expect(cartesSeries({ global: { current: 0, best: 0 }, entracte: null, sillon: null, messaging: { current: 0, best: 0 } })).toEqual([])
  })

  it('trophées : récents de la période, en cours par avancement, cachés comptés', () => {
    const v = vueTrophees(trophees(), 'month', AUJOURDHUI)
    expect(v.recents.map((t) => t.key)).toEqual(['encore_un_episode'])
    expect(v.tous.map((t) => t.key)).toEqual(['encore_un_episode', 'premier_jour'])
    expect(v.cachesTrouves.map((t) => t.key)).toEqual(['signature'])
    expect(v.enCours.map((t) => t.key)).toEqual(['cinephile', 'encore_un_episode'])
    expect(v.cachesRestants).toBe(4)
    expect(v.debloques).toBe(3)
    expect(v.total).toBe(8)
    expect(vueTrophees(trophees(), 'month', '2026-08-15').recents).toEqual([])
  })

  it('reconnaît une personne sans aucune activité', () => {
    expect(aucuneActivite(statsVides(null), { unlocked: [], in_progress: [], hidden_remaining: 2 })).toBe(true)
    expect(aucuneActivite(statsVides(), { unlocked: [], in_progress: [], hidden_remaining: 2 })).toBe(false)
    expect(aucuneActivite(null, null)).toBe(false)
  })
})
