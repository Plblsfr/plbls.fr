import { ApiError } from '../../lib/api'
import { regieApi, type Downloads } from './regie'

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
