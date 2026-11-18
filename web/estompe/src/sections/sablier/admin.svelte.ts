import { ApiError } from '../../lib/api'
import { adminApi, type Personne, type Personnes } from './sablier'

export const INJOIGNABLE = 'Les stats sont injoignables pour le moment.'
export const messageDe = (e: unknown): string => (e instanceof ApiError && e.status >= 400 && e.status < 500 && e.message ? e.message : INJOIGNABLE)

export class EtatAdmin {
  personnes = $state.raw<Personnes | null>(null)
  erreur = $state('')

  constructor(private api: Pick<typeof adminApi, 'personnes' | 'effacer'> = adminApi) {}

  async charger() {
    this.erreur = ''
    try { this.personnes = await this.api.personnes() } catch (e) { this.erreur = messageDe(e) }
  }

  trouver(sub: string): Personne | undefined { return this.personnes?.people.find((p) => p.sub === sub) }

  async effacer(sub: string) {
    await this.api.effacer(sub)
    await this.charger()
  }

  reinitialiser() { this.personnes = null; this.erreur = '' }
}

export const admin = new EtatAdmin()
