import { describe, expect, it } from 'vitest'
import { adminSection, adminSections, autorise, rightsOf } from './admin'

describe("sections d'administration", () => {
  it('suivent les droits du compte', () => {
    expect(adminSections(['permissions']).map((s) => s.id)).toEqual(['permissions'])
    expect(adminSections(['access']).map((s) => s.id)).toEqual([])
    const tout = adminSections(['admin', 'permissions']).map((s) => s.id)
    expect(tout[0]).toBe('users')
    expect(tout.at(-1)).toBe('permissions')
  })
  it('masquent une section dont le bloc est inactif', () => {
    expect(adminSections(['admin'], (b) => b !== 'films').some((s) => s.id === 'films')).toBe(false)
  })
  it('retombent sur la première section permise', () => {
    const seules = adminSections(['permissions'])
    expect(adminSection('users', seules)).toBe('permissions')
    expect(adminSection(undefined, adminSections(['admin', 'permissions']))).toBe('users')
    expect(adminSection('permissions', adminSections(['admin', 'permissions']))).toBe('permissions')
    expect(adminSection('users', [])).toBeUndefined()
  })
  it('se déduisent des anciens champs de /me', () => {
    expect(rightsOf({ activated: true, allowed: true, admin: true, movies: false })).toEqual(['access', 'admin'])
    expect(rightsOf({ activated: false, allowed: false, rights: ['permissions'] })).toEqual(['permissions'])
  })

  it('filtre les sections selon les capacités fines et la section accès', () => {
    expect(adminSections(['acces']).map((s) => s.id)).toEqual(['acces'])
    const caps = { 'estompe:admin.users': { allow: ['*'], deny: [] }, 'estompe:admin.badges': { allow: ['*'], deny: ['*'] } }
    expect(adminSections(['admin'], () => true, caps).map((s) => s.id)).toEqual(['users'])
    expect(adminSections(['admin'], () => true, null).length).toBeGreaterThan(5)
  })

  it('applique la règle du refus qui l’emporte', () => {
    const caps = { 'estompe:vocal.screen': { allow: ['*'], deny: ['srv-1'] }, 'estompe:films.use': { allow: ['lib'], deny: [] } }
    expect(autorise(caps, 'estompe:vocal.screen', 'srv-2')).toBe(true)
    expect(autorise(caps, 'estompe:vocal.screen', 'srv-1')).toBe(false)
    expect(autorise(caps, 'estompe:films.use')).toBe(false)
    expect(autorise(caps, 'estompe:gifs.use')).toBe(false)
    expect(autorise(null, 'estompe:gifs.use')).toBe(true)
  })
})
