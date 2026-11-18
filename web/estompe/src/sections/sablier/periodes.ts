import { ajouterJours, deMois, jourSansAnnee, majuscule, nomMois, versDate, versJour } from './formats'
import type { Periode } from './sablier'

export const PERIODES: { id: Periode; label: string }[] = [
  { id: 'week', label: 'Semaine' },
  { id: 'month', label: 'Mois' },
  { id: 'year', label: 'Année' },
  { id: 'all', label: 'Tout' },
]

export const estPeriode = (v: unknown): v is Periode => PERIODES.some((p) => p.id === v)

export function debutDe(periode: Periode, jour: string): string {
  const d = versDate(jour)
  if (periode === 'week') return ajouterJours(jour, -((d.getUTCDay() + 6) % 7))
  if (periode === 'month') return versJour(new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1)))
  if (periode === 'year') return versJour(new Date(Date.UTC(d.getUTCFullYear(), 0, 1)))
  return jour
}

export function finDe(periode: Periode, jour: string): string {
  const d = versDate(jour)
  if (periode === 'week') return ajouterJours(debutDe('week', jour), 6)
  if (periode === 'month') return versJour(new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0)))
  if (periode === 'year') return versJour(new Date(Date.UTC(d.getUTCFullYear(), 11, 31)))
  return jour
}

export const peutReculer = (periode: Periode, jour: string, premierJour: string | null): boolean =>
  periode !== 'all' && !!premierJour && debutDe(periode, jour) > premierJour

export const peutAvancer = (periode: Periode, jour: string, aujourdhui: string): boolean => periode !== 'all' && finDe(periode, jour) < aujourdhui

export function precedente(periode: Periode, jour: string, premierJour: string | null): string {
  if (!peutReculer(periode, jour, premierJour)) return jour
  return ajouterJours(debutDe(periode, jour), -1)
}

export function suivante(periode: Periode, jour: string, aujourdhui: string): string {
  if (!peutAvancer(periode, jour, aujourdhui)) return jour
  const fin = finDe(periode, ajouterJours(finDe(periode, jour), 1))
  return fin > aujourdhui ? aujourdhui : fin
}

export function borner(jour: string, premierJour: string | null, aujourdhui: string): string {
  if (jour > aujourdhui) return aujourdhui
  if (premierJour && jour < premierJour) return premierJour > aujourdhui ? aujourdhui : premierJour
  return jour
}

export const enCours = (periode: Periode, jour: string, aujourdhui: string): boolean => periode === 'all' || !peutAvancer(periode, jour, aujourdhui)

export function libelle(periode: Periode, jour: string): string {
  const annee = versDate(jour).getUTCFullYear()
  if (periode === 'week') {
    const [du, au] = [debutDe('week', jour), finDe('week', jour)]
    const memeMois = du.slice(0, 7) === au.slice(0, 7)
    return `${memeMois ? String(versDate(du).getUTCDate()) : jourSansAnnee(du)} – ${jourSansAnnee(au)} ${versDate(au).getUTCFullYear()}`
  }
  if (periode === 'month') return `${majuscule(nomMois(jour))} ${annee}`
  if (periode === 'year') return String(annee)
  return 'Depuis le début'
}

export interface Titre { avant: string; fort: string }

export function titre(periode: Periode, jour: string): Titre {
  if (periode === 'week') return { avant: 'Ta ', fort: 'semaine' }
  if (periode === 'month') {
    const de = deMois(jour)
    const coupe = de.startsWith('d’') ? 2 : 3
    return { avant: `Ton mois ${de.slice(0, coupe)}`, fort: de.slice(coupe) }
  }
  if (periode === 'year') return { avant: 'Ton année ', fort: String(versDate(jour).getUTCFullYear()) }
  return { avant: 'Depuis le ', fort: 'début' }
}

export function intitule(periode: Periode, jour: string, aujourdhui: string): string {
  if (periode === 'all') return 'Depuis le début'
  if (enCours(periode, jour, aujourdhui)) return periode === 'week' ? 'Cette semaine' : periode === 'month' ? 'Ce mois-ci' : 'Cette année'
  if (periode === 'week') return `Semaine du ${libelle('week', jour)}`
  return libelle(periode, jour)
}

export function enPeriode(periode: Periode, jour: string): string {
  if (periode === 'week') return 'cette semaine-là'
  if (periode === 'month') return `en ${nomMois(jour)}`
  if (periode === 'year') return `en ${versDate(jour).getUTCFullYear()}`
  return ''
}

const NOMS: Record<Periode, [string, string]> = {
  week: ['Semaine précédente', 'Semaine suivante'],
  month: ['Mois précédent', 'Mois suivant'],
  year: ['Année précédente', 'Année suivante'],
  all: ['', ''],
}
export const nomFleches = (periode: Periode) => NOMS[periode]

export const contient = (periode: Periode, reference: string, jour: string): boolean =>
  periode === 'all' || (jour >= debutDe(periode, reference) && jour <= finDe(periode, reference))

export function libelleRecap(genre: 'month' | 'year', periode: string): string {
  if (genre === 'year') return periode.slice(0, 4)
  return libelle('month', `${periode.slice(0, 7)}-01`)
}
