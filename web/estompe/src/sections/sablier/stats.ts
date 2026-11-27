import { ajouterJours, avancement, duree, jourCourt, jourLong, joursEntre, MOIS_COURTS, nombre, part, pluriel, versDate } from './formats'
import { contient, debutDe } from './periodes'
import type { Domaine, Entracte, JourDuree, Messagerie, Periode, Salon, SansAcces, Serie, Series, Sillon, Stats, TropheeDebloque, TropheeEnCours, Trophees, Usage } from './sablier'

export type NomSalon = (serveur: string, salon: string) => string | null

export const SALON_SUPPRIME = 'salon supprimé'
export const SALON_INACCESSIBLE = 'salon supprimé ou inaccessible'
export const salonTexte = (s: Salon, nom: NomSalon, absent = SALON_SUPPRIME): string => {
  const n = nom(s.server, s.channel)
  return n ? `# ${n}` : absent
}
export const salonVocal = (s: Salon, nom: NomSalon, absent = SALON_SUPPRIME): string => {
  const n = nom(s.server, s.channel)
  return n ? `salon « ${n} »` : absent
}

export const NOM_DOMAINE: Record<Domaine, string> = { usage: 'Utilisation', entracte: 'Entracte', sillon: 'Sillon', messaging: 'Messagerie et vocal' }
export const ICONE_DOMAINE = { usage: 'horloge', entracte: 'film', sillon: 'note', messaging: 'bulle' } as const

const NOM_SECTION: Record<string, string> = { messagerie: 'Messagerie', entracte: 'Entracte', sillon: 'Sillon', regie: 'Régie', sablier: 'Stats', admin: 'Administration', autre: 'Autre' }
const NOM_PLATEFORME: Record<string, string> = { web: 'Web', pwa: 'App web', windows: 'Windows', android: 'Android' }

export interface Tuile { label: string; valeur: string }
export interface Ligne { label: string; valeur: string }
export interface Part { label: string; part: number; valeur: string }
export interface Barre { hauteur: number; actif: boolean }

export interface Carte {
  domaine: Domaine
  etat: 'pleine' | 'vide' | 'sans-acces'
  chiffre?: { valeur: string; unite: string }
  barres?: { liste: Barre[]; description: string }
  tuiles: Tuile[]
  parts: Part[]
  lignes: Ligne[]
  etiquettes: string[]
  note?: string
}

const carte = (domaine: Domaine, etat: Carte['etat']): Carte => ({ domaine, etat, tuiles: [], parts: [], lignes: [], etiquettes: [] })
const garder = <T extends { n: number }>(liste: T[]): T[] => liste.filter((x) => x.n > 0)

const JOURS_MIN_BARRES = 5

export function barresParJour(usage: Pick<Usage, 'period' | 'from' | 'to' | 'by_day'>, aujourdhui: string): Barre[] {
  if (usage.period !== 'week' && usage.period !== 'month') return []
  const fin = usage.to > aujourdhui ? aujourdhui : usage.to
  const jours = joursEntre(usage.from, fin)
  if (jours < JOURS_MIN_BARRES) return []
  const parJour = new Map(usage.by_day.map((j) => [j.day, j.seconds]))
  const max = Math.max(1, ...usage.by_day.map((j) => j.seconds))
  return Array.from({ length: jours }, (_, i) => {
    const s = parJour.get(ajouterJours(usage.from, i)) ?? 0
    return { hauteur: s > 0 ? Math.max(8, Math.round((s / max) * 100)) : 4, actif: s > 0 }
  })
}

export function carteUsage(u: Usage, aujourdhui: string): Carte {
  if (u.seconds <= 0) return carte('usage', 'vide')
  const c = carte('usage', 'pleine')
  c.chiffre = { valeur: duree(u.seconds), unite: 'de temps actif' }
  const barres = barresParJour(u, aujourdhui)
  const fin = u.to > aujourdhui ? aujourdhui : u.to
  const ecoules = joursEntre(u.from, fin)
  if (barres.length && u.by_day.length >= 2) c.barres = { liste: barres, description: `Temps actif par jour du ${jourCourt(u.from)} au ${jourCourt(fin)}, ${pluriel(u.active_days, 'jour actif', 'jours actifs')}` }
  if (u.active_days > 0) c.lignes.push({ label: 'Jours actifs', valeur: u.period === 'all' || ecoules < u.active_days ? nombre(u.active_days) : `${nombre(u.active_days)} sur ${nombre(ecoules)}` })
  if (u.longest_day && u.longest_day.seconds > 0 && u.active_days >= 2) c.lignes.push({ label: 'Journée la plus longue', valeur: `${jourCourt(u.longest_day.day)} · ${duree(u.longest_day.seconds)}` })
  if (u.period === 'all' && u.first_day) c.lignes.push({ label: 'Premier jour d’activité', valeur: jourLong(u.first_day) })
  const sections = u.by_section.filter((s) => s.seconds > 0)
  if (sections.length >= 2) c.parts = sections.map((s) => ({ label: NOM_SECTION[s.section] ?? s.section, part: part(s.seconds, u.seconds), valeur: `${part(s.seconds, u.seconds)} %` })).filter((p) => p.part > 0)
  const plateformes = u.by_platform.filter((p) => p.seconds > 0)
  if (plateformes.length >= 2) c.etiquettes = plateformes.map((p) => ({ nom: NOM_PLATEFORME[p.platform] ?? p.platform, part: part(p.seconds, u.seconds) })).filter((p) => p.part > 0).map((p) => `${p.nom} ${p.part} %`)
  return c
}

export function carteEntracte(e: Entracte | SansAcces): Carte {
  if (!e.access) return carte('entracte', 'sans-acces')
  const rien = e.seconds <= 0 && e.movies_watched <= 0 && e.episodes_watched <= 0 && e.series_completed <= 0 && e.cinema_sessions <= 0 && e.cinema_seconds <= 0
  if (rien) return carte('entracte', 'vide')
  const c = carte('entracte', 'pleine')
  if (e.seconds > 0) c.chiffre = { valeur: duree(e.seconds), unite: 'regardées' }
  c.tuiles = garder([
    { label: e.movies_watched >= 2 ? 'Films vus' : 'Film vu', n: e.movies_watched },
    { label: e.episodes_watched >= 2 ? 'Épisodes vus' : 'Épisode vu', n: e.episodes_watched },
    { label: e.series_completed >= 2 ? 'Séries terminées' : 'Série terminée', n: e.series_completed },
  ]).map((t) => ({ label: t.label, valeur: nombre(t.n) }))
  if (e.longest_session > 0) c.lignes.push({ label: 'Plus longue session', valeur: duree(e.longest_session) })
  if (e.biggest_day && e.biggest_day.seconds > 0 && e.biggest_day.seconds < e.seconds) c.lignes.push({ label: 'Plus grosse journée', valeur: `${jourCourt(e.biggest_day.day)} · ${duree(e.biggest_day.seconds)}` })
  if (e.most_episodes_in_a_row >= 2) c.lignes.push({ label: 'Épisodes enchaînés', valeur: `${nombre(e.most_episodes_in_a_row)} d’un coup` })
  if (e.cinema_sessions > 0 || e.cinema_seconds > 0) {
    const morceaux = [e.cinema_sessions > 0 ? pluriel(e.cinema_sessions, 'séance à plusieurs', 'séances à plusieurs') : '', e.cinema_seconds > 0 ? duree(e.cinema_seconds) : ''].filter(Boolean)
    c.lignes.push({ label: 'Salon cinéma', valeur: morceaux.join(' · ') })
  }
  if (e.intros_skipped > 0) c.lignes.push({ label: 'Intros passées', valeur: nombre(e.intros_skipped) })
  if (e.abandoned > 0) c.lignes.push({ label: 'Titres abandonnés avant la fin', valeur: nombre(e.abandoned) })
  return c
}

export function carteSillon(s: Sillon | SansAcces): Carte {
  if (!s.access) return carte('sillon', 'sans-acces')
  if (s.seconds <= 0 && s.tracks_played <= 0) return carte('sillon', 'vide')
  const c = carte('sillon', 'pleine')
  if (s.seconds > 0) c.chiffre = { valeur: duree(s.seconds), unite: 'd’écoute' }
  c.tuiles = garder([
    { label: s.tracks_played >= 2 ? 'Titres écoutés' : 'Titre écouté', n: s.tracks_played },
    { label: s.distinct_tracks >= 2 ? 'Titres différents' : 'Titre différent', n: s.distinct_tracks === s.tracks_played ? 0 : s.distinct_tracks },
  ]).map((t) => ({ label: t.label, valeur: nombre(t.n) }))
  return c
}

const SALONS_DETAILLES = 3

export function partsMessages(m: Messagerie, nom: NomSalon, absent = SALON_SUPPRIME): Part[] {
  const salons = m.by_channel.filter((s) => s.messages > 0)
  const tete = salons.slice(0, SALONS_DETAILLES)
  const autres = salons.slice(SALONS_DETAILLES).reduce((n, s) => n + s.messages, 0)
  const lignes = [
    ...tete.map((s) => ({ label: salonTexte(s, nom, absent), n: s.messages })),
    { label: 'Privés', n: m.private },
    { label: 'Autres salons', n: autres },
  ].filter((l) => l.n > 0)
  if (lignes.length < 2) return []
  const max = Math.max(...lignes.map((l) => l.n))
  return lignes.map((l) => ({ label: l.label, part: Math.max(2, Math.round((l.n / max) * 100)), valeur: nombre(l.n) }))
}

export function carteMessagerie(m: Messagerie, nom: NomSalon, absent = SALON_SUPPRIME): Carte {
  const rien = m.total <= 0 && m.voice_seconds <= 0 && m.reactions_given <= 0 && m.reactions_received <= 0 && m.files <= 0 && m.screen_share_seconds <= 0
  if (rien) return carte('messaging', 'vide')
  const c = carte('messaging', 'pleine')
  if (m.total > 0) c.chiffre = { valeur: nombre(m.total), unite: m.total >= 2 ? 'messages envoyés' : 'message envoyé' }
  else if (m.voice_seconds > 0) c.chiffre = { valeur: duree(m.voice_seconds), unite: 'en vocal' }
  c.parts = partsMessages(m, nom, absent)
  if (!c.parts.length && m.top_channel && m.top_channel.messages > 0 && m.top_channel.messages < m.total) c.lignes.push({ label: 'Salon le plus fréquenté', valeur: `${salonTexte(m.top_channel, nom, absent)} · ${nombre(m.top_channel.messages)}` })
  if (m.busiest_day && m.busiest_day.messages > 0 && m.busiest_day.messages < m.total) c.lignes.push({ label: 'Journée la plus bavarde', valeur: `${jourCourt(m.busiest_day.day)} · ${pluriel(m.busiest_day.messages, 'message', 'messages')}` })
  if (m.reactions_given > 0 || m.reactions_received > 0) {
    const morceaux = [m.reactions_given > 0 ? `${nombre(m.reactions_given)} ${m.reactions_given >= 2 ? 'données' : 'donnée'}` : '', m.reactions_received > 0 ? `${nombre(m.reactions_received)} ${m.reactions_received >= 2 ? 'reçues' : 'reçue'}` : ''].filter(Boolean)
    c.lignes.push({ label: 'Réactions', valeur: morceaux.join(' · ') })
  }
  if (m.top_emoji && m.top_emoji.count >= 2) c.lignes.push({ label: 'Emoji le plus utilisé', valeur: `${m.top_emoji.emoji} · ${nombre(m.top_emoji.count)} fois` })
  if (m.files > 0) c.lignes.push({ label: m.files >= 2 ? 'Fichiers et images envoyés' : 'Fichier ou image envoyé', valeur: nombre(m.files) })
  if (m.voice_seconds > 0 && m.total > 0) c.lignes.push({ label: 'En vocal', valeur: m.top_voice_channel ? `${duree(m.voice_seconds)} · ${salonVocal(m.top_voice_channel, nom, absent)}` : duree(m.voice_seconds) })
  else if (m.voice_seconds > 0 && m.top_voice_channel) c.lignes.push({ label: 'Salon vocal préféré', valeur: salonVocal(m.top_voice_channel, nom, absent) })
  if (m.longest_call > 0) c.lignes.push({ label: 'Plus long appel', valeur: duree(m.longest_call) })
  if (m.screen_share_seconds > 0) c.lignes.push({ label: 'Partage d’écran', valeur: duree(m.screen_share_seconds) })
  return c
}

export function cartes(stats: Stats, nom: NomSalon, aujourdhui: string, absent = SALON_SUPPRIME): Carte[] {
  const liste = [carteUsage(stats.usage, aujourdhui), carteEntracte(stats.entracte), carteSillon(stats.sillon), carteMessagerie(stats.messaging, nom, absent)]
  const rang = { pleine: 0, vide: 1, 'sans-acces': 2 }
  return liste.map((c, i) => ({ c, i })).sort((a, b) => rang[a.c.etat] - rang[b.c.etat] || a.i - b.i).map((x) => x.c)
}

export const estVide = (stats: Stats, domaine: Domaine): boolean => {
  if (domaine === 'usage') return carteUsage(stats.usage, stats.usage.to).etat === 'vide'
  if (domaine === 'entracte') return carteEntracte(stats.entracte).etat === 'vide'
  if (domaine === 'sillon') return carteSillon(stats.sillon).etat === 'vide'
  return carteMessagerie(stats.messaging, () => null).etat === 'vide'
}

export const TEXTE_VIDE: Record<Domaine, { rien: string; jamais: string }> = {
  usage: { rien: 'Pas de temps actif', jamais: 'Pas encore de temps actif' },
  entracte: { rien: 'Rien regardé', jamais: 'Rien regardé pour l’instant' },
  sillon: { rien: 'Rien écouté', jamais: 'Rien écouté pour l’instant' },
  messaging: { rien: 'Ni message ni vocal', jamais: 'Ni message ni vocal pour l’instant' },
}

export const SANS_ACCES: Partial<Record<Domaine, { titre: string; texte: string }>> = {
  entracte: { titre: 'Pas d’accès à Entracte', texte: 'Ton compte n’a pas encore accès aux films et aux séries. Demande la permission « Entracte » à un administrateur de plbls.fr.' },
  sillon: { titre: 'Pas d’accès à Sillon', texte: 'Ton compte n’a pas encore accès à la musique. Demande la permission « Sillon » à un administrateur de plbls.fr.' },
}

export interface CarteSerie { cle: 'global' | 'entracte' | 'sillon' | 'messaging'; titre: string; icone: 'flamme' | 'film' | 'note' | 'bulle'; etat: 'en-cours' | 'arretee'; nombre: string; unite: string; record: string }

export function cartesSeries(s: Series): CarteSerie[] {
  const sources: { cle: CarteSerie['cle']; titre: string; icone: CarteSerie['icone']; serie: Serie | null }[] = [
    { cle: 'global', titre: 'Toutes activités', icone: 'flamme', serie: s.global },
    { cle: 'entracte', titre: 'Entracte', icone: 'film', serie: s.entracte },
    { cle: 'sillon', titre: 'Sillon', icone: 'note', serie: s.sillon },
    { cle: 'messaging', titre: 'Messagerie', icone: 'bulle', serie: s.messaging },
  ]
  const liste: CarteSerie[] = []
  for (const { cle, titre, icone, serie } of sources) {
    if (!serie || (serie.current <= 0 && serie.best <= 0)) continue
    const record = `Record : ${pluriel(serie.best, 'jour', 'jours')}`
    if (serie.current > 0) liste.push({ cle, titre, icone, etat: 'en-cours', nombre: nombre(serie.current), unite: cle === 'global' ? (serie.current >= 2 ? 'jours d’affilée' : 'jour') : serie.current >= 2 ? 'jours' : 'jour', record })
    else liste.push({ cle, titre, icone, etat: 'arretee', nombre: '', unite: '', record })
  }
  return liste.sort((a, b) => Number(a.etat === 'arretee') - Number(b.etat === 'arretee'))
}

export interface VueTrophees {
  recents: TropheeDebloque[]
  tous: TropheeDebloque[]
  enCours: TropheeEnCours[]
  cachesTrouves: TropheeDebloque[]
  cachesRestants: number
  debloques: number
  total: number
}

export function vueTrophees(t: Trophees, periode: Periode, jour: string): VueTrophees {
  const parDate = t.unlocked.toSorted((a, b) => (a.unlocked_at < b.unlocked_at ? 1 : a.unlocked_at > b.unlocked_at ? -1 : 0))
  const visibles = parDate.filter((x) => !x.hidden)
  const acquis = new Set(t.unlocked.map((x) => x.id))
  return {
    recents: periode === 'all' ? [] : visibles.filter((x) => contient(periode, jour, x.unlocked_at)),
    tous: visibles,
    enCours: t.in_progress.toSorted((a, b) => b.value / Math.max(1, b.next_threshold) - a.value / Math.max(1, a.next_threshold)),
    cachesTrouves: parDate.filter((x) => x.hidden),
    cachesRestants: Math.max(0, t.hidden_remaining),
    debloques: t.unlocked.length,
    total: t.unlocked.length + t.in_progress.filter((x) => !acquis.has(x.id)).length + Math.max(0, t.hidden_remaining),
  }
}

export const pourcentage = (v: number, seuil: number): number => (seuil > 0 ? Math.max(0, Math.min(100, Math.round((v / seuil) * 100))) : 0)
export const texteAvancement = (t: { value: number; next_threshold: number; unit: TropheeEnCours['unit'] }): string => avancement(t.value, t.next_threshold, t.unit)
export const textePalier = (t: { tier: number; tiers: number }): string => (t.tiers > 1 ? `Palier ${t.tier} sur ${t.tiers}` : '')
export const estExploit = (t: { tiers: number }): boolean => t.tiers <= 1
export const dateDeblocage = (jour: string, aujourdhui: string): string => {
  const d = versDate(jour)
  const memeAnnee = d.getUTCFullYear() === versDate(aujourdhui).getUTCFullYear()
  return `le ${memeAnnee ? jourCourt(jour).split(' ').slice(1).join(' ') : jourLong(jour)}`
}

export type Matiere = 'aucune' | 'bronze' | 'argent' | 'or' | 'exploit'
export const MATIERES: Exclude<Matiere, 'aucune'>[] = ['bronze', 'argent', 'or', 'exploit']
export const NOM_MATIERE: Record<Matiere, string> = { aucune: 'Pas encore de médaille', bronze: 'Bronze', argent: 'Argent', or: 'Or', exploit: 'Exploit' }
const VERS: Record<Matiere, string> = { aucune: '', bronze: 'Vers le bronze', argent: 'Vers l’argent', or: 'Vers l’or', exploit: 'Vers l’exploit' }

export const matiereDe = (t: { tier: number; tiers: number; hidden?: boolean }): Matiere => {
  if (t.tier <= 0) return 'aucune'
  if (t.hidden || t.tiers <= 1) return 'exploit'
  return t.tier === 1 ? 'bronze' : t.tier === 2 ? 'argent' : 'or'
}

export interface Piece {
  id: number
  key: string
  nom: string
  description: string
  icone: string
  matiere: Matiere
  vers: Matiere
  cible: string
  pct: number | null
  avancement: string
  palier: string
  palierVise: number
  debloque: string | null
  cache: boolean
}

export interface Collection {
  pieces: Piece[]
  proches: Piece[]
  compte: { matiere: Matiere; nom: string; n: number }[]
  cachesRestants: number
  debloques: number
  total: number
}

type Base = { id: number; key: string; name: string; description: string; icon: string; tier: number; tiers: number }

function piece(t: Base, p: { value: number; next_threshold: number; unit: TropheeEnCours['unit'] } | undefined, debloque: string | null, cache: boolean): Piece {
  const matiere = matiereDe({ tier: t.tier, tiers: t.tiers, hidden: cache })
  const suite = !!p && p.next_threshold > 0
  const vers = suite ? matiereDe({ tier: t.tier + 1, tiers: t.tiers, hidden: cache }) : matiere
  return {
    id: t.id, key: t.key, nom: t.name, description: t.description.replace(/\.$/, ''), icone: t.icon, matiere, vers,
    cible: !suite ? '' : vers === matiere ? `Vers le palier ${t.tier + 1}` : VERS[vers],
    pct: suite && p ? pourcentage(p.value, p.next_threshold) : null,
    avancement: suite && p ? texteAvancement(p) : '',
    palier: t.tier > 0 ? textePalier(t) : '',
    palierVise: t.tier + 1,
    debloque,
    cache,
  }
}

export function collection(t: Trophees): Collection {
  const vue = vueTrophees(t, 'all', '')
  const suites = new Map(t.in_progress.map((x) => [x.id, x]))
  const acquis = new Set(t.unlocked.map((x) => x.id))
  const pieces = [
    ...[...vue.tous, ...vue.cachesTrouves].map((u) => piece(u, u.progress ?? suites.get(u.id), u.unlocked_at, u.hidden)),
    ...t.in_progress.filter((e) => !acquis.has(e.id)).map((e) => piece(e, e, null, false)),
  ].sort((a, b) => (b.pct ?? -1) - (a.pct ?? -1))
  return {
    pieces,
    proches: pieces.filter((p) => p.pct !== null).slice(0, 3),
    compte: MATIERES.map((m) => ({ matiere: m, nom: m === 'exploit' ? 'Exploits' : NOM_MATIERE[m], n: pieces.filter((p) => p.matiere === m).length })),
    cachesRestants: vue.cachesRestants,
    debloques: vue.debloques,
    total: vue.total,
  }
}

export interface CaseJour { jour: string; niveau: 0 | 1 | 2 | 3 | 4; futur: boolean; aujourdhui: boolean; titre: string }
export interface GrilleActivite { cases: CaseJour[]; mois: string[]; actifs: number; du: string }

export function grilleActivite(jours: JourDuree[], aujourdhui: string, semaines = 53): GrilleActivite {
  const du = ajouterJours(debutDe('week', aujourdhui), -(semaines - 1) * 7)
  const parJour = new Map(jours.filter((j) => j.seconds > 0 && j.day >= du && j.day <= aujourdhui).map((j) => [j.day, j.seconds]))
  const max = Math.max(1, ...parJour.values())
  const cases: CaseJour[] = Array.from({ length: semaines * 7 }, (_, i) => {
    const jour = ajouterJours(du, i)
    const s = parJour.get(jour) ?? 0
    const futur = jour > aujourdhui
    return {
      jour, futur, aujourdhui: jour === aujourdhui,
      niveau: s > 0 ? (Math.min(4, Math.max(1, Math.ceil((s / max) * 4))) as CaseJour['niveau']) : 0,
      titre: futur ? jourCourt(jour) : `${jourCourt(jour)} · ${s > 0 ? duree(s) : 'pas d’activité'}`,
    }
  })
  let precedent = -1
  const mois = Array.from({ length: semaines }, (_, s) => {
    const d = versDate(ajouterJours(du, s * 7))
    const m = d.getUTCMonth()
    const nouveau = s === 0 ? d.getUTCDate() <= 7 : m !== precedent
    precedent = m
    return nouveau ? MOIS_COURTS[m] : ''
  })
  return { cases, mois, actifs: parJour.size, du }
}

export interface JourCalendrier { jour: string; numero: number; etat: 'serie' | 'actif' | 'vide' | 'futur'; aujourdhui: boolean }

export function quatreSemaines(jours: JourDuree[], aujourdhui: string, serie: number): JourCalendrier[] {
  const du = ajouterJours(debutDe('week', aujourdhui), -21)
  const actifs = new Set(jours.filter((j) => j.seconds > 0).map((j) => j.day))
  const enSerie = new Set<string>()
  let jour = actifs.has(aujourdhui) ? aujourdhui : ajouterJours(aujourdhui, -1)
  while (enSerie.size < serie && actifs.has(jour)) {
    enSerie.add(jour)
    jour = ajouterJours(jour, -1)
  }
  return Array.from({ length: 28 }, (_, i) => {
    const j = ajouterJours(du, i)
    return { jour: j, numero: versDate(j).getUTCDate(), etat: j > aujourdhui ? 'futur' : enSerie.has(j) ? 'serie' : actifs.has(j) ? 'actif' : 'vide', aujourdhui: j === aujourdhui }
  })
}

export const aucuneActivite = (stats: Stats | null, trophees: Trophees | null): boolean => !!stats && !!trophees && stats.usage.first_day === null && trophees.unlocked.length === 0

export const PLUS_LARGE: Record<Periode, Periode | null> = { week: 'month', month: 'year', year: 'all', all: null }
