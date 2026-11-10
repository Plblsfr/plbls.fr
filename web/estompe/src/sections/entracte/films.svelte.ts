import { filmsApi } from '../../lib/api'
import type { FilmItem } from './films'

class FilmsState {
  movies = $state(0)
  series = $state(0)
  seen = $state(0)
  resume = $state<FilmItem[]>([])

  async refresh() {
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
  }
}

export const films = new FilmsState()
