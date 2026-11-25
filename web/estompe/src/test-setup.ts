import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/svelte'
import { platform } from '@platform'
import { setPlatform } from './lib/platform'

setPlatform(platform)
afterEach(() => cleanup())

const pistesDeTexte = new WeakMap<HTMLMediaElement, EventTarget>()
Object.defineProperty(HTMLMediaElement.prototype, 'textTracks', {
  configurable: true,
  get(this: HTMLMediaElement) {
    let liste = pistesDeTexte.get(this)
    if (!liste) { liste = Object.assign(new EventTarget(), { length: 0 }); pistesDeTexte.set(this, liste) }
    return liste
  },
})
