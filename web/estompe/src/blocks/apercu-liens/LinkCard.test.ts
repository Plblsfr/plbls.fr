import { fireEvent, render } from '@testing-library/svelte'
import { describe, expect, it, vi } from 'vitest'
import { api, filmsApi } from '../../lib/api'
import LinkCard from './LinkCard.svelte'

describe('LinkCard YouTube', () => {
  it('affiche le titre de la vidéo', async () => {
    vi.spyOn(api, 'linkPreview').mockResolvedValue({ url: 'https://youtu.be/aaaaaaaaaaa', title: 'Ma vidéo', site_name: 'YouTube' } as any)
    const { findByText } = render(LinkCard, { url: 'https://youtu.be/aaaaaaaaaaa' })
    await findByText('Ma vidéo')
  })

  it("affiche la miniature et ne charge l'iframe qu'au clic", async () => {
    const { getByLabelText, container } = render(LinkCard, { url: 'https://youtu.be/dQw4w9WgXcQ' })
    expect(container.querySelector('img')?.getAttribute('src')).toContain('/vi/dQw4w9WgXcQ/')
    expect(container.querySelector('iframe')).toBeNull()
    await fireEvent.click(getByLabelText('Lire la vidéo YouTube'))
    expect(container.querySelector('iframe')?.getAttribute('src')).toContain('youtube-nocookie.com/embed/dQw4w9WgXcQ')
  })

  it('agrandit la vidéo dans une fenêtre et Échap la referme', async () => {
    const { getByLabelText } = render(LinkCard, { url: 'https://youtu.be/dQw4w9WgXcQ' })
    await fireEvent.click(getByLabelText('Agrandir la vidéo'))
    expect(document.querySelector('.lightbox iframe')?.getAttribute('src')).toContain('youtube-nocookie.com/embed/dQw4w9WgXcQ')
    await fireEvent.keyDown(window, { key: 'Escape' })
    expect(document.querySelector('.lightbox')).toBeNull()
  })
})

describe('LinkCard Sillon', () => {
  const carte = (o: object) => ({ url: 'https://estompe.plbls.fr/#/musique/album/a1', title: 'Lemonade', description: 'Beyoncé', site_name: 'Sillon', sillon: { type: 'album', id: 'a1', title: 'Lemonade', artist: 'Beyoncé', album: 'a1', cover: null, listen: true, ...o } })

  it('affiche titre et artiste, et « Écouter » ouvre la section', async () => {
    const spy = vi.spyOn(api, 'linkPreview').mockResolvedValue(carte({}) as any)
    const { findByText, getByText } = render(LinkCard, { url: 'https://estompe.plbls.fr/#/musique/album/a1', message: '42' })
    await findByText('Lemonade')
    expect(getByText('Beyoncé')).toBeTruthy()
    expect(spy).toHaveBeenCalledWith('https://estompe.plbls.fr/#/musique/album/a1', '42')
    expect(getByText('Écouter').closest('a')?.getAttribute('href')).toBe('#/musique/album/a1')
  })

  it("ouvre l'album d'un titre et masque « Écouter » sans accès", async () => {
    vi.spyOn(api, 'linkPreview').mockResolvedValue(carte({ type: 'track', id: 't1', title: 'Formation', album: 'a2', listen: false }) as any)
    const { findByText, queryByText } = render(LinkCard, { url: 'https://estompe.plbls.fr/#/musique/track/t1', message: '43' })
    await findByText('Formation')
    expect(queryByText('Écouter')).toBeNull()
    vi.spyOn(api, 'linkPreview').mockResolvedValue(carte({ type: 'track', id: 't1', title: 'Formation 2', album: 'a2' }) as any)
    const r = render(LinkCard, { url: 'https://estompe.plbls.fr/#/musique/track/t1', message: '44' })
    expect((await r.findByText('Écouter')).closest('a')?.getAttribute('href')).toBe('#/musique/album/a2')
  })
})

describe('LinkCard badge d’Entracte', () => {
  it('joue la carte animée du badge, avec la fiche pour qui a Entracte', async () => {
    const url = 'https://estompe.plbls.fr/#/films/badge/season%3As1%3A2'
    vi.spyOn(api, 'linkPreview').mockResolvedValue({
      url, title: 'La Série', description: 'Saison 2 terminée', site_name: 'Entracte',
      entracte_badge: { kind: 'season', title_id: 's1', season: 2, title: 'La Série', season_title: null, year: 2019, poster: false, season_poster: false, episodes: 8, duration: 21600, obtained_at: '2026-11-26T23:10:00.000Z' },
    })
    const { container } = render(LinkCard, { url, message: '43' })
    await vi.waitFor(() => expect(container.querySelector('.lien-badge .film-badge-anim canvas')?.getAttribute('aria-label')).toBe('Saison 2 terminée : La Série'))
    expect(container.querySelector('.lien-badge-fiche')).toBeNull()
    const { app } = await import('../../lib/store.svelte')
    app.entracteMe = { permissions: { access: true, request: false, admin: false } } as never
    const vue = render(LinkCard, { url, message: '44' })
    expect((await vue.findAllByText('Voir la fiche dans Entracte'))[0].closest('a')?.getAttribute('href')).toBe('#/films/item/s1')
    app.entracteMe = null
  })
  const badge = (o: object = {}) => ({ kind: 'season' as const, title_id: 'p1', season: 1, title: 'La Série', season_title: null, year: 2019, poster: false, season_poster: false, episodes: 8, duration: 21600, obtained_at: '2026-11-26T23:10:00.000Z', ...o })

  it('signale la carte intégrée pour masquer le lien du message, pas pour un autre aperçu', async () => {
    const url = 'https://estompe.plbls.fr/#/films/badge/season%3Ap1%3A1'
    vi.spyOn(api, 'linkPreview').mockResolvedValue({ url, title: 'La Série', entracte_badge: badge() } as any)
    const integre = vi.fn()
    render(LinkCard, { url, message: '45', onintegre: integre })
    await vi.waitFor(() => expect(integre).toHaveBeenCalledWith(true))
    vi.spyOn(api, 'linkPreview').mockResolvedValue({ url: 'https://exemple.fr', title: 'Exemple' } as any)
    const autre = vi.fn()
    render(LinkCard, { url: 'https://exemple.fr', message: '46', onintegre: autre })
    await vi.waitFor(() => expect(autre).toHaveBeenCalledWith(false))
  })

  it("charge l'affiche sans repasser par une URL blob (refusée par la CSP)", async () => {
    const url = 'https://estompe.plbls.fr/#/films/badge/movie%3Ap2'
    vi.spyOn(api, 'linkPreview').mockResolvedValue({ url, title: 'Le Film', entracte_badge: badge({ kind: 'movie', title_id: 'p2', season: null, poster: true }) } as any)
    const blob = vi.spyOn(filmsApi, 'imageBlob').mockResolvedValue(new Blob(['x']))
    const bitmap = vi.fn().mockResolvedValue({ width: 2, height: 3 })
    vi.stubGlobal('createImageBitmap', bitmap)
    const recherche = vi.spyOn(globalThis, 'fetch')
    const base = filmsApi.base
    Object.assign(filmsApi, { base: 'https://entracte.test' })
    render(LinkCard, { url, message: '47' })
    await vi.waitFor(() => expect(bitmap).toHaveBeenCalled())
    expect(blob).toHaveBeenCalledWith('p2', 'poster', 500)
    expect(recherche).not.toHaveBeenCalled()
    Object.assign(filmsApi, { base })
    vi.unstubAllGlobals()
  })

  it('avec les animations réduites, ne joue pas seule mais se lance au clic', async () => {
    const url = 'https://estompe.plbls.fr/#/films/badge/season%3Ap3%3A1'
    vi.spyOn(api, 'linkPreview').mockResolvedValue({ url, title: 'La Série', entracte_badge: badge({ title_id: 'p3' }) } as any)
    vi.stubGlobal('matchMedia', (q: string) => ({ matches: q.includes('reduce'), media: q, addEventListener() {}, removeEventListener() {} }))
    const { findByLabelText, queryByLabelText } = render(LinkCard, { url, message: '48' })
    await fireEvent.click(await findByLabelText('Lire l’animation'))
    expect(queryByLabelText('Lire l’animation')).toBeNull()
    vi.unstubAllGlobals()
  })
})
