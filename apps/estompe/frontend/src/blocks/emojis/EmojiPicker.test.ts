import { fireEvent, render, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import EmojiPicker from './EmojiPicker.svelte'

beforeEach(() => localStorage.clear())

describe('EmojiPicker', () => {
  it("donne le focus à la recherche à l'ouverture et affiche les catégories", async () => {
    const { getByLabelText, getByRole, findByText } = render(EmojiPicker, { onpick: () => {}, onclose: () => {} })
    expect(document.activeElement).toBe(getByLabelText('Rechercher un smiley'))
    await findByText('Smileys et émotions')
    expect(getByRole('group', { name: 'Catégories' }).querySelectorAll('button').length).toBe(9) // pas de « Récents » tant que rien n'a été utilisé
  })

  it('cherche par nom, code ou mot-clé et insère avec Entrée ou Maj+clic sans fermer', async () => {
    const onpick = vi.fn()
    const { getByLabelText, findByLabelText } = render(EmojiPicker, { onpick, onclose: () => {} })
    await fireEvent.input(getByLabelText('Rechercher un smiley'), { target: { value: ':joy:' } })
    const b = await findByLabelText('visage riant aux larmes')
    await fireEvent.click(b, { shiftKey: true })
    expect(onpick).toHaveBeenLastCalledWith('😂', true)
    await fireEvent.keyDown(getByLabelText('Rechercher un smiley'), { key: 'Enter' })
    expect(onpick).toHaveBeenLastCalledWith('😂', false)
  })

  it('range le smiley choisi dans les récents et affiche le nom et le code au survol', async () => {
    const { getByLabelText, findByLabelText, getByRole, container } = render(EmojiPicker, { onpick: () => {}, onclose: () => {} })
    await fireEvent.input(getByLabelText('Rechercher un smiley'), { target: { value: 'joy' } })
    const b = await findByLabelText('visage riant aux larmes')
    await fireEvent.mouseOver(b)
    expect(container.querySelector('.ep-foot')?.textContent).toContain(':joy:')
    await fireEvent.click(b)
    await fireEvent.input(getByLabelText('Rechercher un smiley'), { target: { value: '' } })
    await waitFor(() => expect(getByRole('group', { name: 'Catégories' }).querySelector('button')?.getAttribute('aria-label')).toBe('Récents'))
  })

  it('se ferme avec Échap', async () => {
    const onclose = vi.fn()
    const { getByRole } = render(EmojiPicker, { onpick: () => {}, onclose })
    await fireEvent.keyDown(getByRole('dialog'), { key: 'Escape' })
    expect(onclose).toHaveBeenCalled()
  })
})
