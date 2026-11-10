import { fireEvent, render } from '@testing-library/svelte'
import { describe, expect, it, vi } from 'vitest'
import Lightbox from './Lightbox.svelte'

describe('Lightbox', () => {
  it('prend le focus, le garde dans la fenêtre et le rend à la fermeture', async () => {
    const origine = document.createElement('button')
    document.body.append(origine)
    origine.focus()
    const { unmount } = render(Lightbox, { src: 'blob:x', name: 'photo.png', onclose: vi.fn() })
    const box = document.querySelector<HTMLElement>('.lightbox')!
    expect(document.activeElement).toBe(box)
    const [telecharger, fermer] = [box.querySelector<HTMLElement>('a')!, box.querySelector<HTMLElement>('button')!]
    await fireEvent.keyDown(box, { key: 'Tab' })
    expect(document.activeElement).toBe(telecharger)
    await fireEvent.keyDown(box, { key: 'Tab' })
    expect(document.activeElement).toBe(fermer)
    await fireEvent.keyDown(box, { key: 'Tab' })
    expect(document.activeElement).toBe(telecharger)
    await fireEvent.keyDown(box, { key: 'Tab', shiftKey: true })
    expect(document.activeElement).toBe(fermer)
    unmount()
    expect(document.activeElement).toBe(origine)
    origine.remove()
  })
})
