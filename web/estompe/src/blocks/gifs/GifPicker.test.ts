import { render } from '@testing-library/svelte'
import { describe, expect, it, vi } from 'vitest'
import { api } from '../../lib/api'
import GifPicker from './GifPicker.svelte'

describe('GifPicker', () => {
  it("donne le focus au champ de recherche à l'ouverture", () => {
    vi.spyOn(api, 'gifSearch').mockResolvedValue([])
    const { getByLabelText } = render(GifPicker, { onpick: () => {}, onclose: () => {} })
    expect(document.activeElement).toBe(getByLabelText('Rechercher un GIF'))
  })

  it('se décale pour rester dans la fenêtre', () => {
    vi.spyOn(api, 'gifSearch').mockResolvedValue([])
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ left: -50 } as DOMRect)
    const { getByRole } = render(GifPicker, { onpick: () => {}, onclose: () => {} })
    expect(getByRole('dialog').style.transform).toBe('translateX(58px)')
  })
})
