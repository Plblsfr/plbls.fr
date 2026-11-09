import { describe, expect, it } from 'vitest'
import catalogue from '../../../../api/estompe/core/src/catalogue.rs?raw'
import { ADMIN_SECTIONS } from './admin'

const cles = new Set([...catalogue.matchAll(/^\s*e\(\s*"([a-z0-9._-]+)"/gm)].map((m) => m[1]))
const sources = import.meta.glob<string>(['../**/*.svelte', '../**/*.ts', '!../**/*.test.ts', '!../vendor/**'], { query: '?raw', import: 'default', eager: true })

describe('permissions utilisées par le front', () => {
  it('lit le catalogue de l’API', () => {
    expect(cles.size).toBeGreaterThan(40)
    expect(cles.has('access')).toBe(true)
    expect(Object.keys(sources).length).toBeGreaterThan(100)
  })

  it('n’utilise que des permissions du catalogue', () => {
    const inconnues = Object.entries(sources).flatMap(([f, texte]) =>
      [...texte.matchAll(/\bpeut\(\s*'([^']+)'/g)].map((m) => m[1]).filter((k) => !cles.has(k)).map((k) => `${f}: ${k}`),
    )
    expect(inconnues).toEqual([])
  })

  it('chaque section d’administration suit une permission du catalogue', () => {
    for (const s of ADMIN_SECTIONS) {
      if ('cap' in s) expect(cles.has(s.cap.replace(/^estompe:/, '')), s.cap).toBe(true)
    }
  })
})
