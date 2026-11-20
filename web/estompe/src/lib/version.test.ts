import { describe, it, expect } from 'vitest'
// @ts-expect-error pas de @types/node dans le projet : le test tourne sous Node via vitest
import { readFileSync } from 'node:fs'
import { bump, concerned, nextVersions, pendingVersions, remplacerVersionCargo } from '../../../../scripts/estompe/version.mjs'
import { policy } from '../../../../scripts/estompe/update-policy.mjs'

const merge = (b: string) => `Merge pull request #1 from Plblsfr/${b}`

describe('éléments concernés', () => {
  it('un dossier = un élément', () => {
    expect(concerned(['api/estompe/core/src/x.rs'])).toEqual(['api'])
    expect(concerned(['apps/estompe/windows/src-tauri/a.rs'])).toEqual(['windows'])
    expect(concerned(['web/estompe/plateforme/docker/default.conf'])).toEqual(['web'])
  })
  it('web/estompe/ : web, windows et android ; crates/ : windows et android', () => {
    expect(concerned(['web/estompe/src/a.ts']).sort()).toEqual(['android', 'web', 'windows'])
    expect(concerned(['apps/estompe/crates/core/src/lib.rs']).sort()).toEqual(['android', 'windows'])
  })
  it('app native : natif/, crates/client et crates/plateforme → natif seulement', () => {
    expect(concerned(['apps/estompe/natif/src/main.rs'])).toEqual(['natif'])
    expect(concerned(['apps/estompe/crates/client/src/lib.rs'])).toEqual(['natif'])
    expect(concerned(['apps/estompe/crates/plateforme/src/lib.rs'])).toEqual(['natif'])
    expect(concerned(['apps/estompe/natif/installeur/estompe-natif.nsi', 'apps/estompe/crates/core/src/lib.rs']).sort()).toEqual(['android', 'natif', 'windows'])
  })
  it('docs, CI, autres projets et .version : aucun', () => expect(concerned(['docs/estompe/a.md', '.github/workflows/x.yml', '.version', 'web/autre/src/a.ts', 'api/autre/src/main.rs'])).toEqual([]))
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
    expect(nextVersions(v, merge('feat/a'), ['api/estompe/core/src/x.rs'])).toEqual({ ...v, api: '0.2.0' })
    expect(nextVersions(v, merge('fix/a'), ['web/estompe/src/a.ts'])).toEqual({ ...v, web: '0.2.1', windows: '0.3.1', android: '0.4.1' })
    expect(nextVersions(v, merge('fix/a'), ['docs/estompe/a.md'])).toEqual(v)
  })
  it('app native : un .version sans natif part de 0.0.0', () => {
    const v = { api: '0.1.0', web: '0.2.0', windows: '0.3.0', android: '0.4.0' }
    expect(nextVersions(v, merge('feat/natif-bureau'), ['apps/estompe/natif/src/main.rs'])).toEqual({ ...v, natif: '0.1.0' })
    expect(nextVersions({ ...v, natif: '0.1.0' }, merge('fix/natif'), ['apps/estompe/crates/client/src/lib.rs'])).toEqual({ ...v, natif: '0.1.1' })
  })
  it('les fusions en attente se cumulent dans l\'ordre', () => {
    const v = { api: '0.1.0', web: '0.2.0', windows: '0.3.0', android: '0.4.0' }
    const api = ['api/estompe/core/src/x.rs']
    expect(pendingVersions(v, [{ subject: merge('feat/a'), files: api }, { subject: merge('feat/b'), files: api }, { subject: merge('fix/c'), files: api }])).toEqual({ ...v, api: '0.3.1' })
    expect(pendingVersions(v, [{ subject: merge('fix/a'), files: api }, { subject: merge('ci/version'), files: ['.version'] }, { subject: merge('feat/b'), files: ['web/estompe/src/a.ts'] }])).toEqual({ api: '0.1.1', web: '0.3.0', windows: '0.4.0', android: '0.5.0' })
    expect(pendingVersions(v, [])).toEqual(v)
  })
})

describe('fichier .version', () => {
  const versions = JSON.parse(readFileSync('../../.version', 'utf8'))
  it('liste la version de chaque élément', () => expect(Object.keys(versions).filter((k) => k !== 'natif').sort()).toEqual(['android', 'api', 'web', 'windows']))
  it('au format x.y.z', () => { for (const v of Object.values(versions)) expect(v).toMatch(/^\d+\.\d+\.\d+$/) })
})

describe('version des fichiers Cargo de l\'app native', () => {
  it('remplace la version du seul paquet visé', () => {
    const lock = '[[package]]\nname = "estompe-client"\nversion = "0.1.0"\n\n[[package]]\nname = "estompe-natif"\nversion = "0.1.0"\ndependencies = [\n "estompe-client",\n]\n'
    expect(remplacerVersionCargo(lock, 'estompe-natif', '0.2.0')).toBe(lock.replace('name = "estompe-natif"\nversion = "0.1.0"', 'name = "estompe-natif"\nversion = "0.2.0"'))
    expect(remplacerVersionCargo('[package]\nname = "estompe-natif"\nversion = "0.1.0"\nedition.workspace = true\n', 'estompe-natif', '1.0.0')).toBe('[package]\nname = "estompe-natif"\nversion = "1.0.0"\nedition.workspace = true\n')
  })
  it('échoue si le paquet est absent', () => expect(() => remplacerVersionCargo('[package]\nname = "autre"\nversion = "0.1.0"\n', 'estompe-natif', '0.2.0')).toThrow())
  it('politique de mise à jour de natif', () => {
    expect(policy('natif', { mandatory: false, minVersion: '', message: '', platforms: { natif: { mandatory: true, minVersion: '0.3.0' } } })).toEqual({ mandatory: true, minVersion: '0.3.0', message: '', force: true })
    expect(policy('natif', { mandatory: false, platforms: { windows: { mandatory: true } } }).mandatory).toBe(false)
  })
})
