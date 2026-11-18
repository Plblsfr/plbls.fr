import { entracteApi, filmsApi } from '../../lib/api'
import type { FilmItem } from './films'

class FilmsState {
  movies = $state(0)
  series = $state(0)
  seen = $state(0)
  resume = $state<FilmItem[]>([])
  query = $state('')
  pending = $state(0)

  async refresh(admin = false) {
    const [movies, series, seen, resume] = await Promise.all([
      filmsApi.library({ kind: 'movie', limit: '1' }),
      filmsApi.library({ kind: 'series', limit: '1' }),
      filmsApi.seen(),
      filmsApi.resume(),
    ])
    this.movies = movies.total
    this.series = series.total
    this.seen = seen.items.length
    this.resume = resume.items
    if (admin) this.pending = (await entracteApi.adminRequests({ state: 'en_attente' }).catch(() => ({ items: [] }))).items.length
  }
}

export const films = new FilmsState()
