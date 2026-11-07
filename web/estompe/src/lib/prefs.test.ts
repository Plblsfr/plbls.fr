import { afterEach, describe, expect, it } from 'vitest'
import { applyMotion, prefs } from './prefs.svelte'

describe('réduire les animations', () => {
  afterEach(() => { prefs.reduceMotion = false; prefs.save() })

  it('suit le système par défaut', () => {
    expect(prefs.reduceMotion).toBe(false)
    expect(document.documentElement.dataset.motion).toBeUndefined()
  })
  it('pose data-motion="reduit" sur la racine à l\'enregistrement, et le retire', () => {
    prefs.reduceMotion = true
    prefs.save()
    expect(document.documentElement.dataset.motion).toBe('reduit')
    expect(JSON.parse(localStorage.getItem('estompe.prefs') ?? '{}').reduceMotion).toBe(true)
    prefs.reduceMotion = false
    prefs.save()
    expect(document.documentElement.dataset.motion).toBeUndefined()
  })
  it('agit sur l\'élément donné', () => {
    const el = document.createElement('div')
    applyMotion(true, el)
    expect(el.dataset.motion).toBe('reduit')
    applyMotion(false, el)
    expect(el.dataset.motion).toBeUndefined()
  })
})
