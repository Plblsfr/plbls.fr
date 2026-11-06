import { describe, expect, it } from 'vitest'
// @ts-expect-error pas de @types/node dans le projet : le test tourne sous Node via vitest
import { readFileSync } from 'node:fs'
import { startViewport } from './viewport'

const lire = (f: string): string => readFileSync(new URL(f, import.meta.url), 'utf8')
const css = lire('../app.css')
const html = lire('../../index.html')

describe('champs de saisie tactiles', () => {
  it('forcent 16 px sur écran tactile seulement', () => {
    expect(css).toMatch(/@media \(pointer: coarse\) \{ input, textarea, select \{ font-size: 16px !important; \} \}/)
  })

  it('ne bloquent pas le zoom de l’utilisateur', () => {
    expect(html).not.toMatch(/maximum-scale|user-scalable/)
  })
})

describe('startViewport', () => {
  it('publie la hauteur du viewport visuel', () => {
    const vv = Object.assign(new EventTarget(), { height: 400 })
    Object.defineProperty(window, 'visualViewport', { value: vv, configurable: true })
    startViewport()
    expect(document.documentElement.style.getPropertyValue('--vvh')).toBe('400px')
    vv.height = 700
    vv.dispatchEvent(new Event('resize'))
    expect(document.documentElement.style.getPropertyValue('--vvh')).toBe('700px')
  })
})
