import { describe, expect, it } from 'vitest'
import { outranks, groupMembers, nameColor, rolesOf, topPosition, PERM_GROUPS, isTimedOut, shownName } from './roles'
import type { Role, ServerDetail } from './types'

const role = (id: string, position: number, o: Partial<Role> = {}): Role => ({ id, name: id, color: '', icon: '', position, hoist: false, permissions: [], is_default: false, ...o })
const detail = (roles: Role[], members: ServerDetail['members']): ServerDetail => ({ server: { id: 's', name: 's', owner_id: 'a', icon_version: 0 }, channels: [], members, roles })
const m = (id: string, roles: string[] = []) => ({ id, pseudo: id, online: false, roles })

describe('rôles', () => {
  const d = detail([role('admin', 3, { color: '#ff0000', hoist: true }), role('dj', 2, { color: '#00ff00' }), role('ami', 1, { hoist: true }), role('e', 0, { is_default: true })], [])
  it('classe les rôles d\'un membre du plus élevé au plus bas', () => {
    expect(rolesOf(d, m('x', ['ami', 'admin', 'dj'])).map((r) => r.id)).toEqual(['admin', 'dj', 'ami'])
    expect(topPosition(d, m('x', ['ami', 'dj']))).toBe(2)
    expect(topPosition(d, m('x'))).toBe(0)
  })
  it('prend la couleur du rôle le plus élevé qui en a une', () => {
    expect(nameColor(d, m('x', ['ami', 'dj']))).toBe('#00ff00')
    expect(nameColor(d, m('x', ['ami']))).toBeUndefined()
    expect(nameColor(d, m('x', ['dj', 'admin']))).toBe('#ff0000')
  })
  it('groupe les membres par rôle affiché séparément, le plus élevé d\'abord', () => {
    const groups = groupMembers(d, [m('a', ['ami']), m('b', ['dj']), m('c', ['ami', 'admin']), m('d')])
    expect(groups.map((g) => [g.role?.id ?? null, g.members.map((x) => x.id)])).toEqual([['admin', ['c']], ['ami', ['a']], [null, ['b', 'd']]])
  })
  it('ne crée pas de groupe vide', () => {
    expect(groupMembers(d, [m('a')]).map((g) => g.role)).toEqual([null])
  })
  it('décrit 25 permissions, chacune une seule fois', () => {
    const keys = PERM_GROUPS.flatMap((g) => g.perms.map((p) => p.key))
    expect(keys).toHaveLength(25)
    expect(new Set(keys).size).toBe(25)
  })
  it('ne gère que les membres sous son rôle le plus élevé, jamais le créateur', () => {
    const dd = detail(d.roles!, [m('a', ['admin']), m('b', ['dj']), m('c', ['dj']), m('o')])
    dd.server.owner_id = 'o'
    expect(outranks(dd, 'a', 'b')).toBe(true)
    expect(outranks(dd, 'b', 'a')).toBe(false)
    expect(outranks(dd, 'b', 'c')).toBe(false)
    expect(outranks(dd, 'a', 'a')).toBe(false)
    expect(outranks(dd, 'a', 'o')).toBe(false)
    expect(outranks(dd, 'o', 'a')).toBe(true)
  })
})

describe('modération', () => {
  it('reconnaît une exclusion temporaire en cours', () => {
    const now = Date.parse('2026-10-26T12:00:00Z')
    expect(isTimedOut({ timeout_until: '2026-10-26T12:00:01Z' }, now)).toBe(true)
    expect(isTimedOut({ timeout_until: '2026-10-26T12:00:00Z' }, now)).toBe(false)
    expect(isTimedOut({ timeout_until: null }, now)).toBe(false)
    expect(isTimedOut(undefined, now)).toBe(false)
  })
  it('préfère le pseudo de serveur', () => {
    expect(shownName({ pseudo: 'bob', nickname: 'Bobby' })).toBe('Bobby')
    expect(shownName({ pseudo: 'bob', nickname: '' })).toBe('bob')
    expect(shownName({ pseudo: 'bob' })).toBe('bob')
  })
})
