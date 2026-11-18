import type { Unite } from './sablier'

export const MOIS = ['janvier', 'février', 'mars', 'avril', 'mai', 'juin', 'juillet', 'août', 'septembre', 'octobre', 'novembre', 'décembre']
const MOIS_COURTS = ['janv.', 'févr.', 'mars', 'avr.', 'mai', 'juin', 'juil.', 'août', 'sept.', 'oct.', 'nov.', 'déc.']
const JOURS = ['dimanche', 'lundi', 'mardi', 'mercredi', 'jeudi', 'vendredi', 'samedi']
const JOURS_COURTS = ['dim.', 'lun.', 'mar.', 'mer.', 'jeu.', 'ven.', 'sam.']

const JOUR_MS = 86_400_000
const deux = (n: number) => String(n).padStart(2, '0')

export const versDate = (jour: string): Date => {
  const [a, m, j] = jour.slice(0, 10).split('-').map(Number)
  return new Date(Date.UTC(a, (m || 1) - 1, j || 1))
}
export const versJour = (d: Date): string => `${d.getUTCFullYear()}-${deux(d.getUTCMonth() + 1)}-${deux(d.getUTCDate())}`
export const ajouterJours = (jour: string, n: number): string => versJour(new Date(versDate(jour).getTime() + n * JOUR_MS))
export const joursEntre = (du: string, au: string): number => Math.round((versDate(au).getTime() - versDate(du).getTime()) / JOUR_MS) + 1
export const jourLocal = (d: Date = new Date()): string => `${d.getFullYear()}-${deux(d.getMonth() + 1)}-${deux(d.getDate())}`

const quantieme = (n: number) => (n === 1 ? '1er' : String(n))
export const majuscule = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)
export const nomMois = (jour: string) => MOIS[versDate(jour).getUTCMonth()]
export const deMois = (jour: string) => (/^[aeiouyéèêàâîôûh]/i.test(nomMois(jour)) ? `d’${nomMois(jour)}` : `de ${nomMois(jour)}`)
export const enMois = (jour: string) => `en ${nomMois(jour)}`

export const jourCourt = (jour: string) => {
  const d = versDate(jour)
  return `${JOURS_COURTS[d.getUTCDay()]} ${quantieme(d.getUTCDate())} ${MOIS_COURTS[d.getUTCMonth()]}`
}
export const jourSansAnnee = (jour: string) => {
  const d = versDate(jour)
  return `${quantieme(d.getUTCDate())} ${MOIS_COURTS[d.getUTCMonth()]}`
}
export const jourMoyen = (jour: string) => `${jourSansAnnee(jour)} ${versDate(jour).getUTCFullYear()}`
export const jourLong = (jour: string) => {
  const d = versDate(jour)
  return `${quantieme(d.getUTCDate())} ${MOIS[d.getUTCMonth()]} ${d.getUTCFullYear()}`
}
export const jourDeSemaine = (jour: string) => {
  const d = versDate(jour)
  return `${JOURS[d.getUTCDay()]} ${quantieme(d.getUTCDate())}`
}
export const jourEtMois = (jour: string) => {
  const d = versDate(jour)
  return `${quantieme(d.getUTCDate())} ${MOIS[d.getUTCMonth()]}`
}

const ESPACE = ' '

export const nombre = (n: number): string => String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ESPACE)

export const pluriel = (n: number, un: string, plusieurs: string): string => `${nombre(n)} ${n >= 2 ? plusieurs : un}`

export function duree(secondes: number): string {
  const minutes = Math.floor(Math.max(0, secondes) / 60)
  if (minutes < 1) return secondes > 0 ? 'moins d’une minute' : '0 min'
  if (minutes < 60) return `${minutes} min`
  const heures = Math.floor(minutes / 60)
  const reste = minutes % 60
  if (heures >= 100 || reste === 0) return `${nombre(heures)} h`
  return `${heures} h ${deux(reste)}`
}

export function dureeRonde(secondes: number): string {
  const minutes = Math.round(Math.max(0, secondes) / 60)
  if (minutes < 60) return `${Math.max(1, minutes)} min`
  return `${nombre(Math.round(minutes / 60))} h`
}

export const part = (valeur: number, total: number): number => (total > 0 ? Math.round((valeur / total) * 100) : 0)

export function valeur(v: number, unite: Unite): string {
  if (unite === 'seconds') return duree(v)
  if (unite === 'days') return pluriel(v, 'jour', 'jours')
  return nombre(v)
}

export function avancement(v: number, seuil: number, unite: Unite): string {
  if (unite === 'seconds') {
    const enHeures = seuil >= 3600
    const diviseur = enHeures ? 3600 : 60
    return `${nombre(Math.floor(v / diviseur))} / ${nombre(Math.round(seuil / diviseur))} ${enHeures ? 'h' : 'min'}`
  }
  if (unite === 'days') return `${nombre(v)} / ${nombre(seuil)} jours`
  return `${nombre(v)} / ${nombre(seuil)}`
}
