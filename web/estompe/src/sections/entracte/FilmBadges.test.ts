import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { api, filmsApi } from '../../lib/api'
import { app } from '../../lib/store.svelte'
import FilmBadges from './FilmBadges.svelte'
import type { FilmBadge } from './badges'

const badge = (b: Partial<FilmBadge>): FilmBadge => ({
  kind: 'movie', title_id: 'f1', season: null, title: 'Le Grand Film', season_title: null, year: 2021, poster: false, season_poster: false,
  episodes: null, duration: 7980, obtained_at: '2026-03-02T20:00:00.000Z', ...b,
})

describe('FilmBadges', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    app.phase = 'loading'
    app.dms = []
    location.hash = ''
  })

  it('liste les badges, les filtre et envoie un badge animé dans une conversation', async () => {
    app.phase = 'ready'
    app.dms = [{ id: 'dm1', other: { id: 'u2', pseudo: 'bob' }, online: false }]
    vi.spyOn(filmsApi, 'badges').mockResolvedValue({ items: [
      badge({ kind: 'season', title_id: 's1', season: 1, title: 'La Série', episodes: 2, duration: 5400 }),
      badge({}),
    ] })
    const envoi = vi.spyOn(api, 'sendDMMessage').mockResolvedValue({} as never)
    const vue = render(FilmBadges)
    await waitFor(() => expect(vue.getByText('Saison 1 terminée')).toBeTruthy())
    expect(vue.queryByRole('button', { name: /^Séries/ })).toBeNull()
    await fireEvent.click(vue.getByRole('button', { name: 'Films · 1' }))
    expect(vue.queryByText('Saison 1 terminée')).toBeNull()
    await fireEvent.click(vue.getByRole('button', { name: 'Film terminé : Le Grand Film. Ouvrir' }))
    await waitFor(() => expect(vue.container.ownerDocument.querySelector('.film-badge-anim canvas')).toBeTruthy())
    expect(vue.getByText('Envoyer dans une conversation')).toBeTruthy()
    await fireEvent.click(vue.getByRole('button', { name: /bob/ }))
    await waitFor(() => expect(envoi).toHaveBeenCalledTimes(1))
    const [id, contenu] = envoi.mock.calls[0]
    expect(id).toBe('dm1')
    expect(contenu).toBe(`Film terminé : Le Grand Film\n${location.origin}${location.pathname}#/films/badge/movie%3Af1`)
  })

  it('sans messagerie, la carte ne propose pas d’envoi', async () => {
    app.phase = 'sections'
    vi.spyOn(filmsApi, 'badges').mockResolvedValue({ items: [badge({})] })
    const vue = render(FilmBadges, { cle: 'movie:f1' })
    await waitFor(() => expect(vue.container.ownerDocument.querySelector('.film-badge-anim')).toBeTruthy())
    expect(vue.queryByText('Envoyer dans une conversation')).toBeNull()
  })

  it('le lien du badge de quelqu’un d’autre ouvre la fiche du titre', async () => {
    vi.spyOn(filmsApi, 'badges').mockResolvedValue({ items: [] })
    render(FilmBadges, { cle: 'season:s9:2' })
    await waitFor(() => expect(location.hash).toBe('#/films/item/s9'))
  })

  it('sans badge, explique comment gagner le premier', async () => {
    vi.spyOn(filmsApi, 'badges').mockResolvedValue({ items: [] })
    const vue = render(FilmBadges)
    await waitFor(() => expect(vue.getByText(/Aucun badge pour l’instant/)).toBeTruthy())
  })
})
