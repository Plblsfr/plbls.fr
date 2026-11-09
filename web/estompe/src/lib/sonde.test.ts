import { afterEach, describe, expect, it, vi } from 'vitest'
import { sonde } from './sonde'

const images = async (n: number) => { for (let i = 0; i < n; i++) await new Promise((ok) => requestAnimationFrame(() => ok(null))) }

describe('sonde de mesure', () => {
  afterEach(() => { sonde.arreter() })

  it('ne mesure rien hors session', async () => {
    const add = vi.spyOn(document, 'addEventListener')
    sonde.reception()
    await images(3)
    expect(add).not.toHaveBeenCalled()
    expect(sonde.arreter()).toEqual({ images: [], messages: [], saisie: [] })
    add.mockRestore()
  })

  it('mesure la latence de réception et de saisie pendant une session, puis retire son écouteur', async () => {
    sonde.demarrer()
    sonde.reception()
    const champ = document.createElement('textarea')
    document.body.append(champ)
    champ.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }))
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'b', bubbles: true }))
    await vi.waitFor(async () => { await images(1); expect(sonde.releve().messages).toHaveLength(1); expect(sonde.releve().saisie).toHaveLength(1) })
    const d = sonde.arreter()
    expect(d.messages[0][1]).toBeGreaterThanOrEqual(0)
    expect(d.saisie).toHaveLength(1)
    champ.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', bubbles: true }))
    await images(3)
    expect(sonde.arreter().saisie).toEqual([])
    champ.remove()
  })

  it('suit les intervalles entre images seulement quand on le demande', async () => {
    sonde.demarrer()
    await images(3)
    sonde.suivreImages(true)
    await vi.waitFor(async () => { await images(1); expect(sonde.releve().images.length).toBeGreaterThan(1) })
    sonde.suivreImages(false)
    const n = sonde.releve().images.length
    await images(3)
    expect(sonde.arreter().images).toHaveLength(n)
  })

  it('note une seule fois la première conversation affichée', async () => {
    sonde.conversationAffichee()
    await vi.waitFor(() => expect(sonde.premiereConversation).not.toBeNull())
    const t = sonde.premiereConversation
    sonde.conversationAffichee()
    await images(3)
    expect(sonde.premiereConversation).toBe(t)
  })
})
