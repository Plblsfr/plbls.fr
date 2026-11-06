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
