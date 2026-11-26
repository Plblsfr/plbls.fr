export type Sens = 'debit' | 'credit'

const FINE = '\u202f'
const INSECABLE = '\u00a0'
const MOINS = '\u2212'

const entier = (n: number) => {
  if (!Number.isSafeInteger(n)) throw new RangeError('montant en centimes entiers attendu')
  return n
}

const chiffres = (centimes: number) => {
  const abs = Math.abs(entier(centimes))
  const unites = String(Math.trunc(abs / 100)).replace(/\B(?=(\d{3})+$)/g, FINE)
  return `${unites},${String(abs % 100).padStart(2, '0')}`
}

export const nombre = (n: number) => String(entier(n)).replace(/\B(?=(\d{3})+$)/g, FINE)

export const euros = (centimes: number) => `${centimes < 0 ? MOINS : ''}${chiffres(centimes)}${INSECABLE}€`

export const eurosSignes = (centimes: number) => `${centimes < 0 ? MOINS : '+'}${chiffres(centimes)}${INSECABLE}€`

export const montant = (centimes: number, sens: Sens) => eurosSignes(sens === 'debit' ? -Math.abs(centimes) : Math.abs(centimes))

export const ecart = (centimes: number) => (centimes === 0 ? `=${INSECABLE}${euros(0)}` : eurosSignes(centimes))

export const signe = (centimes: number, sens: Sens) => (sens === 'debit' ? -Math.abs(entier(centimes)) : Math.abs(entier(centimes)))

const JOUR = /^(\d{4})-(\d{2})-(\d{2})$/

function lire(jour: string): Date | null {
  const m = JOUR.exec(jour)
  if (!m) return null
  const d = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])))
  return d.getUTCMonth() === Number(m[2]) - 1 ? d : null
}

const fmt = (options: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat('fr-FR', { timeZone: 'UTC', ...options })
const SEMAINE = fmt({ weekday: 'long' })
const MOIS_LONG = fmt({ month: 'long' })
const MOIS_COURT = fmt({ month: 'short' })

const quantieme = (d: Date) => (d.getUTCDate() === 1 ? '1er' : String(d.getUTCDate()))
const majuscule = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export const anneeDe = (jour: string) => lire(jour)?.getUTCFullYear() ?? null

export function jourLong(jour: string, anneeCourante?: number): string {
  const d = lire(jour)
  if (!d) return jour
  const texte = `${majuscule(SEMAINE.format(d))} ${quantieme(d)} ${MOIS_LONG.format(d)}`
  return anneeCourante === undefined || anneeCourante === d.getUTCFullYear() ? texte : `${texte} ${d.getUTCFullYear()}`
}

export function dateCourte(jour: string): string {
  const d = lire(jour)
  return d ? `${quantieme(d)} ${MOIS_COURT.format(d)} ${d.getUTCFullYear()}` : jour
}

export function dateLongue(jour: string): string {
  const d = lire(jour)
  return d ? `${SEMAINE.format(d)} ${dateCourte(jour)}` : jour
}

export function periode(du: string, au: string): string {
  const a = lire(du)
  const b = lire(au)
  if (!a || !b) return `${du} – ${au}`
  if (du === au) return `Le ${dateCourte(du)}`
  if (a.getUTCFullYear() !== b.getUTCFullYear()) return `Du ${dateCourte(du)} au ${dateCourte(au)}`
  if (a.getUTCMonth() !== b.getUTCMonth()) return `Du ${quantieme(a)} ${MOIS_COURT.format(a)} au ${dateCourte(au)}`
  return `Du ${quantieme(a)} au ${dateCourte(au)}`
}

export function moisLong(jour: string): string {
  const d = lire(jour)
  return d ? `${MOIS_LONG.format(d)} ${d.getUTCFullYear()}` : jour
}

export function instant(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'short', year: 'numeric' }).format(d).replace(/^1 /, '1er ')
}

export const pluriel = (n: number, un: string, plusieurs: string) => `${nombre(n)} ${n > 1 ? plusieurs : un}`
