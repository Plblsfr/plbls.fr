import { duree, majuscule, nombre } from './formats'
import type { Catalogue, EntreeJournal, ExploitCatalogue, GenreTrophee, GrainTrophee, Personne, SaisieTrophee, StatCatalogue, TropheeAdmin, Unite } from './sablier'

export const ONGLETS = [
  { id: 'personnes', label: 'Personnes' },
  { id: 'totaux', label: 'Totaux' },
  { id: 'journal', label: 'Journal' },
  { id: 'trophees', label: 'Trophées' },
] as const
export type Onglet = (typeof ONGLETS)[number]['id']
export const ongletDe = (id: string | undefined): Onglet => ONGLETS.find((o) => o.id === id)?.id ?? 'personnes'

export const subCourt = (sub: string): string => (sub.length > 10 ? `${sub.slice(0, 4)}…${sub.slice(-2)}` : sub)
export const nomPersonne = (p: Pick<Personne, 'sub' | 'name' | 'known'>): string => p.name?.trim() || `${p.known ? 'Sans nom' : 'Compte supprimé'} · ${subCourt(p.sub)}`
export const nomCible = (e: Pick<EntreeJournal, 'target_sub' | 'target_name'>): string => e.target_name?.trim() || `Sans nom · ${subCourt(e.target_sub)}`

const plat = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase()

export function filtrerPersonnes(personnes: Personne[], recherche: string): Personne[] {
  const q = plat(recherche.trim())
  if (!q) return personnes
  return personnes.filter((p) => plat(nomPersonne(p)).includes(q) || p.sub.toLowerCase().includes(q))
}

export const avecCompte = (personnes: Personne[]): Personne[] => personnes.filter((p) => p.known)
export const sansCompte = (personnes: Personne[]): Personne[] => personnes.filter((p) => !p.known)

export function filtrerJournal(entrees: EntreeJournal[], recherche: string): EntreeJournal[] {
  const q = plat(recherche.trim())
  if (!q) return entrees
  return entrees.filter((e) => plat(e.admin_name).includes(q) || plat(nomCible(e)).includes(q) || e.target_sub.toLowerCase().includes(q))
}

export const NOM_ACTION = { consultation: 'Consultation', effacement: 'Effacement des données' } as const
export const NOM_GRAIN: Record<GrainTrophee, string> = { day: 'Jour', week: 'Semaine', month: 'Mois', all: 'Tout (cumul depuis le début)' }
const GRAIN_COURT: Record<GrainTrophee, string> = { day: 'Jour', week: 'Semaine', month: 'Mois', all: 'Tout' }
export const NOM_DOMAINE_COURT = { usage: 'Utilisation', entracte: 'Entracte', sillon: 'Sillon', messaging: 'Messagerie' } as const
export const UNITE_SAISIE: Record<Unite, string> = { count: '', seconds: 'en heures', days: 'en jours', date: '' }

const HEURE = 3600
export const MAX_PALIERS = 6

export function enSaisie(valeur: number, unite: Unite): string {
  if (unite !== 'seconds') return String(valeur)
  return String(Math.round((valeur / HEURE) * 100) / 100).replace('.', ',')
}

export function depuisSaisie(texte: string, unite: Unite): number | null {
  const propre = texte.trim().replace(/\s/g, '').replace(',', '.')
  if (!/^\d+(\.\d+)?$/.test(propre)) return null
  const n = Number(propre)
  if (unite === 'seconds') return Math.round(n * HEURE)
  return Number.isInteger(n) ? n : null
}

export const texteSeuil = (valeur: number, unite: Unite): string => (unite === 'seconds' ? duree(valeur) : nombre(valeur))

export const estSerie = (stat: string | null | undefined): boolean => !!stat && stat.startsWith('streak.')
export const statDe = (c: Catalogue | null, cle: string | null | undefined): StatCatalogue | undefined => c?.stats.find((s) => s.key === cle)
export const exploitDe = (c: Catalogue | null, cle: string | null | undefined): ExploitCatalogue | undefined => c?.exploits.find((e) => e.key === cle)
export const uniteDe = (c: Catalogue | null, t: Pick<Formulaire, 'genre' | 'stat' | 'exploit'>): Unite =>
  (t.genre === 'tiers' ? statDe(c, t.stat)?.unit : exploitDe(c, t.exploit)?.unit) ?? 'count'

export interface LigneTrophee { type: string; code: boolean; stat: string; periode: string; seuils: string }

export function ligneTrophee(t: TropheeAdmin, c: Catalogue | null): LigneTrophee {
  if (t.kind === 'exploit') {
    const e = exploitDe(c, t.exploit)
    return { type: 'Exploit', code: true, stat: majuscule(e?.label ?? t.exploit ?? ''), periode: '—', seuils: t.parameter !== null ? texteSeuil(t.parameter, e?.unit ?? 'count') : '—' }
  }
  const s = statDe(c, t.stat)
  return {
    type: estSerie(t.stat) ? 'Série de jours' : 'Paliers', code: false, stat: majuscule(s?.label ?? t.stat ?? ''),
    periode: estSerie(t.stat) || !t.period ? '—' : GRAIN_COURT[t.period],
    seuils: t.thresholds.map((v) => texteSeuil(v, s?.unit ?? 'count')).join(' · '),
  }
}

export const resumeTrophees = (liste: TropheeAdmin[]): string => {
  const actifs = liste.filter((t) => t.active).length
  return `${liste.length} ${liste.length >= 2 ? 'trophées' : 'trophée'}, ${actifs} ${actifs >= 2 ? 'actifs' : 'actif'}.`
}

export interface Formulaire {
  cle: string
  nom: string
  description: string
  icone: string
  cache: boolean
  actif: boolean
  genre: GenreTrophee
  stat: string
  periode: GrainTrophee
  seuils: string[]
  exploit: string
  parametre: string
}

export type Champ = 'cle' | 'nom' | 'description' | 'icone' | 'stat' | 'periode' | 'seuils' | 'exploit' | 'parametre'
export type Erreurs = Partial<Record<Champ, string>>

export function cleDepuisNom(nom: string): string {
  return plat(nom).replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '').slice(0, 60)
}

export function formulaireVide(c: Catalogue | null): Formulaire {
  const stat = c?.stats.find((s) => s.tiers)
  const exploit = c?.exploits[0]
  return {
    cle: '', nom: '', description: '', icone: 'trophee', cache: false, actif: true, genre: 'tiers',
    stat: stat?.key ?? '', periode: 'all', seuils: [''], exploit: exploit?.key ?? '',
    parametre: exploit && exploit.default_parameter !== null ? enSaisie(exploit.default_parameter, exploit.unit) : '',
  }
}

export function formulaireDe(t: TropheeAdmin, c: Catalogue | null): Formulaire {
  const vide = formulaireVide(c)
  const unite = t.kind === 'tiers' ? (statDe(c, t.stat)?.unit ?? 'count') : (exploitDe(c, t.exploit)?.unit ?? 'count')
  return {
    cle: t.key, nom: t.name, description: t.description, icone: t.icon, cache: t.hidden, actif: t.active, genre: t.kind,
    stat: t.stat ?? vide.stat, periode: t.period ?? 'all',
    seuils: t.kind === 'tiers' && t.thresholds.length ? t.thresholds.map((v) => enSaisie(v, unite)) : [''],
    exploit: t.exploit ?? vide.exploit,
    parametre: t.parameter !== null ? enSaisie(t.parameter, unite) : '',
  }
}

export function choisirExploit(f: Formulaire, c: Catalogue | null, cle: string): Formulaire {
  const e = exploitDe(c, cle)
  return { ...f, exploit: cle, parametre: e && e.default_parameter !== null ? enSaisie(e.default_parameter, e.unit) : '' }
}

export function choisirStat(f: Formulaire, c: Catalogue | null, cle: string): Formulaire {
  const avant = statDe(c, f.stat)?.unit
  const apres = statDe(c, cle)?.unit
  return { ...f, stat: cle, periode: estSerie(cle) ? 'all' : f.periode, seuils: avant === apres ? f.seuils : [''] }
}

const CLE = /^[a-z0-9_-]{1,60}$/
const ICONE = /^[a-z0-9-]{1,40}$/

export type Resultat = { ok: true; corps: SaisieTrophee } | { ok: false; erreurs: Erreurs }

export function versRequete(f: Formulaire, c: Catalogue | null, icones: readonly string[], original: TropheeAdmin | null = null): Resultat {
  const erreurs: Erreurs = {}
  const nom = f.nom.trim()
  const description = f.description.trim()
  if (!original && !CLE.test(f.cle)) erreurs.cle = 'De 1 à 60 caractères : minuscules sans accent, chiffres, tiret ou tiret bas.'
  if (nom.length < 1 || nom.length > 60) erreurs.nom = 'De 1 à 60 caractères.'
  if (description.length < 1 || description.length > 200) erreurs.description = 'De 1 à 200 caractères.'
  if (!ICONE.test(f.icone) || !icones.includes(f.icone)) erreurs.icone = 'Choisis une icône de la liste.'
  const complet: SaisieTrophee = { key: f.cle, name: nom, description, icon: f.icone, hidden: f.cache, active: f.actif, kind: f.genre }
  if (f.genre === 'tiers') {
    const stat = statDe(c, f.stat)
    if (!stat?.tiers) erreurs.stat = 'Choisis une stat du catalogue.'
    if (!c?.periods.includes(f.periode)) erreurs.periode = 'Choisis une période.'
    const seuils = f.seuils.map((s) => depuisSaisie(s, stat?.unit ?? 'count'))
    if (seuils.length < 1 || seuils.length > MAX_PALIERS) erreurs.seuils = `De 1 à ${MAX_PALIERS} paliers.`
    else if (seuils.some((v) => v === null || v < 1)) erreurs.seuils = stat?.unit === 'seconds' ? 'Chaque palier est un nombre d’heures supérieur à zéro.' : 'Chaque palier est un nombre entier supérieur à zéro.'
    else if (seuils.some((v, i) => i > 0 && (v as number) <= (seuils[i - 1] as number))) erreurs.seuils = 'Les paliers doivent être strictement croissants.'
    complet.stat = f.stat
    complet.period = f.periode
    complet.thresholds = seuils.filter((v): v is number => v !== null)
  } else {
    const exploit = exploitDe(c, f.exploit)
    if (!exploit) erreurs.exploit = 'Choisis un exploit du registre.'
    complet.exploit = f.exploit
    if (exploit && exploit.default_parameter !== null) {
      const p = depuisSaisie(f.parametre, exploit.unit)
      if (p === null || p < 1) erreurs.parametre = exploit.unit === 'seconds' ? 'Un nombre d’heures supérieur à zéro.' : 'Un nombre entier supérieur à zéro.'
      else complet.parameter = p
    }
  }
  if (Object.keys(erreurs).length) return { ok: false, erreurs }
  if (!original) return { ok: true, corps: complet }
  const corps: SaisieTrophee = {}
  if (complet.name !== original.name) corps.name = complet.name
  if (complet.description !== original.description) corps.description = complet.description
  if (complet.icon !== original.icon) corps.icon = complet.icon
  if (complet.hidden !== original.hidden) corps.hidden = complet.hidden
  if (complet.active !== original.active) corps.active = complet.active
  if (f.genre === 'tiers') {
    if (complet.stat !== original.stat) corps.stat = complet.stat
    if (complet.period !== original.period) corps.period = complet.period
    if (JSON.stringify(complet.thresholds) !== JSON.stringify(original.thresholds)) corps.thresholds = complet.thresholds
  } else {
    if (complet.exploit !== original.exploit) corps.exploit = complet.exploit
    if (complet.parameter !== undefined && (complet.parameter !== original.parameter || corps.exploit !== undefined)) corps.parameter = complet.parameter
  }
  return { ok: true, corps }
}

export const ICONES_SUGGEREES = ['trophee', 'film', 'tele', 'casque', 'note', 'bulle', 'flamme', 'etoile', 'horloge', 'calendar', 'mic', 'sourire'] as const
