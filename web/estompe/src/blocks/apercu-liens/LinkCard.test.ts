import { fireEvent, render } from '@testing-library/svelte'
import { describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
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
