import { describe, it, expect } from 'vitest'
// @ts-expect-error pas de @types/node dans le projet : le test tourne sous Node via vitest
import { readFileSync } from 'node:fs'
import { bump, concerned, nextVersions, pendingVersions, SERVICES } from '../../../../scripts/estompe/version.mjs'
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
  it('app native : natif/, crates/client et crates/plateforme → aucun', () => {
    expect(concerned(['apps/estompe/natif/src/main.rs'])).toEqual([])
    expect(concerned(['apps/estompe/crates/client/src/lib.rs'])).toEqual([])
    expect(concerned(['apps/estompe/crates/plateforme/src/lib.rs'])).toEqual([])
    expect(concerned(['apps/estompe/natif/installeur/estompe-natif.nsi', 'apps/estompe/crates/core/src/lib.rs']).sort()).toEqual(['android', 'windows'])
  })
  it('docs, CI, autres projets et .version : aucun', () => expect(concerned(['docs/estompe/a.md', '.github/workflows/x.yml', '.version', 'web/autre/src/a.ts', 'api/autre/src/main.rs', 'Cargo.lock', 'docs/sillon/openapi.yaml'])).toEqual([]))
  it('services : leur dossier, api/commun/ pour chaque service Rust', () => {
    expect(concerned(['api/sillon/src/main.rs', 'packages/ui/src/a.css']).sort()).toEqual(['sillon', 'ui'])
    expect(concerned(['api/security/Cargo.toml'])).toEqual(['security'])
    expect(concerned(['api/commun/src/lib.rs']).sort()).toEqual(['aegis', 'entracte', 'regie', 'sablier', 'security', 'sillon', 'tirelire'])
  })
  it('services : un fichier .md ne compte pas', () => expect(concerned(['api/entracte/README.md', 'packages/ui/CHANGELOG.md', 'api/commun/NOTES.md'])).toEqual([]))
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
  it('app native : aucune version, la clé natif est retirée de .version', () => {
    const v = { api: '0.1.0', web: '0.2.0', windows: '0.3.0', android: '0.4.0' }
    expect(nextVersions(v, merge('feat/natif-bureau'), ['apps/estompe/natif/src/main.rs'])).toEqual(v)
    expect(nextVersions({ ...v, natif: '0.1.0' }, merge('fix/natif'), ['apps/estompe/crates/client/src/lib.rs'])).toEqual(v)
    expect(nextVersions({ ...v, natif: '0.1.0' }, merge('fix/a'), ['docs/estompe/a.md'])).toEqual(v)
  })
  it('services : même règle que les éléments d\'Estompe', () => {
    const v = { api: '0.1.0', sillon: '0.3.0', ui: '2.0.2' }
    expect(nextVersions(v, merge('feat/a'), ['api/sillon/src/x.rs'])).toEqual({ ...v, sillon: '0.4.0' })
    expect(nextVersions(v, merge('fix/a'), ['packages/ui/src/a.css'])).toEqual({ ...v, ui: '2.0.3' })
    expect(nextVersions(v, merge('release/x'), ['api/sillon/src/x.rs'])).toEqual(v)
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
  it('liste la version de chaque élément d\'Estompe', () => expect(['android', 'api', 'web', 'windows'].every((k) => k in versions)).toBe(true))
  it('ne liste que des éléments connus', () => { for (const k of Object.keys(versions)) expect(['android', 'api', 'web', 'windows', 'natif', ...SERVICES]).toContain(k) })
  it('au format x.y.z', () => { for (const v of Object.values(versions)) expect(v).toMatch(/^\d+\.\d+\.\d+$/) })
})

describe('politique de mise à jour', () => {
  it('politique de mise à jour de natif', () => {
    expect(policy('natif', { mandatory: false, minVersion: '', message: '', platforms: { natif: { mandatory: true, minVersion: '0.3.0' } } })).toEqual({ mandatory: true, minVersion: '0.3.0', message: '', force: true })
    expect(policy('natif', { mandatory: false, platforms: { windows: { mandatory: true } } }).mandatory).toBe(false)
  })
})
