import { render } from '@testing-library/svelte'
import { describe, expect, it } from 'vitest'
import Icon, { cleIcone, type IconName, traceIcone } from './Icon.svelte'

const sources = import.meta.glob<string>(['../**/*.svelte', '!../vendor/**'], { query: '?raw', import: 'default', eager: true })

describe('Icon', () => {
  it('trace chaque nom écrit en dur dans le code', () => {
    const noms = new Set<string>()
    for (const src of Object.values(sources)) {
      for (const m of src.matchAll(/<Icon [^>]*?name=(?:"([^"]+)"|\{([^}]+)\})/g)) {
        if (m[1]) noms.add(m[1])
        for (const q of (m[2] ?? '').matchAll(/[?:]\s*'([a-z0-9-]+)'/g)) noms.add(q[1])
      }
    }
    expect(noms.size).toBeGreaterThan(30)
    for (const n of noms) expect(traceIcone(n as IconName), n).toMatch(/^<(path|circle|rect)/)
  })

  it('renvoie les anciens noms vers le jeu v3', () => {
    expect(cleIcone('search')).toBe('cherche')
    expect(traceIcone('pin')).toBe(traceIcone('epingle'))
  })

  it('remplit les icônes pleines et trace les autres au trait de 1,8', () => {
    const plein = render(Icon, { name: 'play' }).container.querySelector('svg')!
    expect(plein.getAttribute('fill')).toBe('currentColor')
    const trait = render(Icon, { name: 'hash' }).container.querySelector('svg')!
    expect(trait.getAttribute('stroke-width')).toBe('1.8')
  })
})
