import { ApiError } from '../../lib/api'
import { CHUNK, regieApi, type Downloads } from './regie'

type Lire = (since?: number) => Promise<Downloads>

/** Suivi en direct des téléchargements : chaque appel attend côté serveur le prochain changement, et le suivant part aussitôt. */
export class Suivi {
  data = $state.raw<Downloads | null>(null)
  failed = $state('')
  private tour = 0

  constructor(private lire: Lire = regieApi.downloads, private pause: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms))) {}

  start() {
    const tour = ++this.tour
    void this.boucle(tour)
  }

  stop() { this.tour++ }

  private async boucle(tour: number) {
    let echecs = 0
    while (tour === this.tour) {
      try {
        const d = await this.lire(this.data?.version)
        if (tour !== this.tour) return
        this.data = d.torrents || !this.data ? d : { ...this.data, version: d.version, error: d.error }
        this.failed = ''
        echecs = 0
      } catch (e) {
        if (tour !== this.tour) return
        if (e instanceof ApiError && (e.status === 401 || e.status === 403)) { this.failed = e.message; return }
        if (!(e instanceof ApiError && e.code === 'timeout')) {
          this.failed = e instanceof Error ? e.message : String(e)
          echecs++
          await this.pause(Math.min(1000 * 2 ** (echecs - 1), 15_000))
        }
      }
    }
  }
}

type Envoyer = (offset: number, morceau: Blob, reset: boolean) => Promise<{ received: number }>

/**
 * Envoie un fichier par morceaux à partir de `recu` (0 avec `remplacer` : le fichier repart du début). Un refus `bad_offset` donne la position
 * à reprendre ; une coupure réseau est retentée trois fois par morceau. `suivre` reçoit le nombre d'octets reçus par le serveur.
 */
export async function envoyerFichier(fichier: Blob, recu: number, envoyer: Envoyer, suivre: (recu: number) => void, options: { remplacer?: boolean; taille?: number; pause?: (ms: number) => Promise<void>; arret?: () => boolean } = {}) {
  const taille = options.taille ?? CHUNK
  const pause = options.pause ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)))
  let position = options.remplacer ? 0 : recu
  let reset = !!options.remplacer
  let echecs = 0
  if (fichier.size === 0 && reset) { await envoyer(0, fichier.slice(0, 0, 'application/octet-stream'), true); suivre(0); return }
  while (position < fichier.size) {
    if (options.arret?.()) throw new ApiError(0, 'cancelled', 'Envoi interrompu.')
    try {
      const r = await envoyer(position, fichier.slice(position, position + taille, 'application/octet-stream'), reset)
      position = r.received
      reset = false
      echecs = 0
    } catch (e) {
      if (e instanceof ApiError && e.code === 'bad_offset' && typeof e.data.received === 'number') { position = e.data.received; reset = false }
      else if (e instanceof ApiError && e.status >= 400 && e.status < 500) throw e
      else if (++echecs > 3) throw e
      else await pause(1000 * 2 ** (echecs - 1))
    }
    suivre(position)
  }
}
