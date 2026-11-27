import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { filmsApi } from '../../lib/api'
import FilmBadges from './FilmBadges.svelte'
import type { FilmBadge } from './badges'

const badge = (b: Partial<FilmBadge>): FilmBadge => ({
  kind: 'movie', title_id: 'f1', season: null, title: 'Le Grand Film', season_title: null, year: 2021, poster: false, season_poster: false,
  episodes: null, duration: 7980, obtained_at: '2026-03-02T20:00:00.000Z', ...b,
})

describe('FilmBadges', () => {
  afterEach(() => vi.restoreAllMocks())

  it('liste les badges, les filtre par genre et ouvre la carte à partager', async () => {
    vi.spyOn(filmsApi, 'badges').mockResolvedValue({ items: [
      badge({ kind: 'season', title_id: 's1', season: 1, title: 'La Série', episodes: 2, duration: 5400 }),
      badge({}),
    ] })
    const vue = render(FilmBadges)
    await waitFor(() => expect(vue.getByText('Saison 1 terminée')).toBeTruthy())
    expect(vue.getByText('Film terminé')).toBeTruthy()
    expect(vue.queryByRole('button', { name: /^Séries/ })).toBeNull()
    await fireEvent.click(vue.getByRole('button', { name: 'Films · 1' }))
    expect(vue.queryByText('Saison 1 terminée')).toBeNull()
    await fireEvent.click(vue.getByRole('button', { name: 'Film terminé : Le Grand Film. Partager' }))
    await waitFor(() => expect(vue.getByText('L’image n’a pas pu être préparée.')).toBeTruthy())
    expect(vue.getByRole('button', { name: /Enregistrer l’image/ }).hasAttribute('disabled')).toBe(true)
  })

  it('sans badge, explique comment gagner le premier', async () => {
    vi.spyOn(filmsApi, 'badges').mockResolvedValue({ items: [] })
    const vue = render(FilmBadges)
    await waitFor(() => expect(vue.getByText(/Aucun badge pour l’instant/)).toBeTruthy())
  })
})
