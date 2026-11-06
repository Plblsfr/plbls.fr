import { describe, it, expect } from 'vitest'
// @ts-expect-error pas de @types/node dans le projet : le test tourne sous Node via vitest
import { readFileSync } from 'node:fs'
import { bump, concerned, nextVersions } from '../../scripts/version.mjs'

const merge = (b: string) => `Merge pull request #1 from Plblsfr/${b}`

describe('éléments concernés', () => {
  it('un dossier = un élément', () => {
    expect(concerned(['backend/core/src/x.rs'])).toEqual(['api'])
    expect(concerned(['windows/src-tauri/a.rs'])).toEqual(['windows'])
    expect(concerned(['web/nginx.conf'])).toEqual(['web'])
  })
  it('frontend/ : web, windows et android ; crates/ : windows et android', () => {
    expect(concerned(['frontend/src/a.ts']).sort()).toEqual(['android', 'web', 'windows'])
    expect(concerned(['crates/core/src/lib.rs']).sort()).toEqual(['android', 'windows'])
  })
  it('docs, CI et .version : aucun', () => expect(concerned(['docs/a.md', '.github/workflows/x.yml', '.version'])).toEqual([]))
})

describe('version', () => {
  it('feat/… : minor +1, patch remis à 0', () => expect(bump('0.2.3', merge('feat/sondages'))).toBe('0.3.0'))
  it('fix/… et autres : patch +1', () => {
    expect(bump('0.2.3', merge('fix/upload'))).toBe('0.2.4')
    expect(bump('0.2.3', merge('chore/ci'))).toBe('0.2.4')
    expect(bump('0.2.3', 'Visionneuse (#11)')).toBe('0.2.4') // squash ou commit direct : le nom de la branche est perdu
  })
  it('release/… : la version ne change pas', () => expect(bump('0.2.3', merge('release/0.2.3'))).toBe('0.2.3'))
  it('compare les nombres, pas le texte', () => expect(bump('0.9.9', merge('fix/x'))).toBe('0.9.10'))
  it('seuls les éléments concernés montent', () => {
    const v = { api: '0.1.0', web: '0.2.0', windows: '0.3.0', android: '0.4.0' }
    expect(nextVersions(v, merge('feat/a'), ['backend/core/src/x.rs'])).toEqual({ ...v, api: '0.2.0' })
    expect(nextVersions(v, merge('fix/a'), ['frontend/src/a.ts'])).toEqual({ ...v, web: '0.2.1', windows: '0.3.1', android: '0.4.1' })
    expect(nextVersions(v, merge('fix/a'), ['docs/a.md'])).toEqual(v)
  })
})

describe('fichier .version', () => {
  const versions = JSON.parse(readFileSync('../.version', 'utf8'))
  it('liste la version de chaque élément', () => expect(Object.keys(versions).sort()).toEqual(['android', 'api', 'web', 'windows']))
  it('au format x.y.z', () => { for (const v of Object.values(versions)) expect(v).toMatch(/^\d+\.\d+\.\d+$/) })
})
