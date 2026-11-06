import { describe, expect, it, beforeEach } from 'vitest'
import { ctx, ctxmenu, placeMenu, selectAll, selectionIn } from './ctxmenu.svelte'
import { placeCard } from './profile.svelte'
import { cropRect } from '../blocks/avatars/avatars.svelte'

describe('placeMenu', () => {
  it('s\'ouvre au curseur quand il y a de la place', () => {
    expect(placeMenu(100, 100, 200, 150, 800, 600)).toEqual({ left: 100, top: 100 })
  })
  it('se replie à gauche et en haut près des bords', () => {
    expect(placeMenu(790, 590, 200, 150, 800, 600)).toEqual({ left: 592, top: 442 })
  })
})

describe('ctx', () => {
  beforeEach(() => (ctxmenu.at = null))
  const item = { label: 'Copier', run: () => {} }
  const right = (el: Element) => { const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 5, clientY: 6 }); el.dispatchEvent(e); return e }

  it('ouvre le menu à la position du curseur et bloque le menu natif', () => {
    const div = document.createElement('div')
    ctx(div, () => [item])
    expect(right(div).defaultPrevented).toBe(true)
    expect(ctxmenu.at).toEqual({ x: 5, y: 6, items: [item] })
  })
  it('garde le menu natif sur un champ de saisie ou un lien', () => {
    const div = document.createElement('div')
    div.innerHTML = '<input><a href="/x">lien</a>'
    ctx(div, () => [item])
    for (const t of ['input', 'a']) expect(right(div.querySelector(t)!).defaultPrevented).toBe(false)
    expect(ctxmenu.at).toBeNull()
  })
  it('garde le menu natif sans entrée', () => {
    const div = document.createElement('div')
    ctx(div, () => [])
    expect(right(div).defaultPrevented).toBe(false)
  })
})

describe('sélection du texte', () => {
  it('sélectionne le contenu d\'un message et le renvoie pour la copie', () => {
    document.body.innerHTML = '<div id="a"><p>bonjour <b>toi</b></p></div><div id="b">autre</div>'
    const a = document.getElementById('a')!
    expect(selectionIn(a)).toBe('')
    selectAll(a)
    expect(selectionIn(a)).toBe('bonjour toi')
    expect(selectionIn(document.getElementById('b')!)).toBe('')
  })
})

describe('carte de profil et recadrage', () => {
  it('place la carte à droite du clic, ou à gauche près du bord, toujours dans la fenêtre', () => {
    expect(placeCard({ x: 100, y: 300 }, 372, 400, 1200, 800)).toEqual({ left: 112, top: 276 })
    expect(placeCard({ x: 1100, y: 700 }, 372, 400, 1200, 800)).toEqual({ left: 716, top: 392 })
  })
  it('recadre au centre par défaut, et selon le zoom et le déplacement', () => {
    expect(cropRect(400, 200)).toEqual({ sx: 100, sy: 0, size: 200 })
    expect(cropRect(400, 200, { zoom: 2, x: -1, y: 1 })).toEqual({ sx: 0, sy: 100, size: 100 })
    expect(cropRect(400, 200, { zoom: 2, x: 1, y: 0 })).toEqual({ sx: 300, sy: 50, size: 100 })
  })
})
