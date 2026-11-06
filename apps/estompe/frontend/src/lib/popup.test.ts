import { describe, expect, it } from 'vitest'
import { placePopup } from './popup'

const vw = 800, vh = 600
describe('placePopup', () => {
  it('s\'aligne sur la réaction quand il y a de la place', () => {
    expect(placePopup({ left: 100, top: 300, right: 160, bottom: 328 }, 200, 100, vw, vh)).toEqual({ left: 100, top: 200 })
  })
  it('reste entièrement visible au bord droit', () => {
    const p = placePopup({ left: 760, top: 300, right: 790, bottom: 328 }, 220, 100, vw, vh)
    expect(p.left + 220).toBeLessThanOrEqual(vw - 8)
  })
  it('s\'ouvre en dessous en haut de la conversation', () => {
    expect(placePopup({ left: 100, top: 20, right: 160, bottom: 48 }, 200, 100, vw, vh).top).toBe(48)
  })
  it('reste dans la fenêtre en bas', () => {
    const p = placePopup({ left: 100, top: 40, right: 160, bottom: 590 }, 200, 100, vw, vh)
    expect(p.top + 100).toBeLessThanOrEqual(vh - 8)
  })
})
