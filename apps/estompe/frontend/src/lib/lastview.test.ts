import { beforeEach, describe, expect, it } from 'vitest'
import { clearLastView, lastViewConv, lastViewHref, saveLastView } from './lastview'

const servers = [{ id: 's1' }]
const dms = [{ id: 'd1' }]

describe('dernière vue', () => {
  beforeEach(() => localStorage.clear())

  it('retrouve le salon et le message privé du compte', () => {
    saveLastView('u1', '#/s/s1/c9')
    expect(lastViewHref('u1', servers, dms)).toBe('#/s/s1/c9')
    saveLastView('u1', '#/dm/d1')
    expect(lastViewHref('u1', servers, dms)).toBe('#/dm/d1')
  })
  it('ignore un autre compte, un serveur disparu, une déconnexion', () => {
    saveLastView('u1', '#/s/s1/c9')
    expect(lastViewHref('u2', servers, dms)).toBeNull()
    expect(lastViewHref('u1', [], dms)).toBeNull()
    clearLastView()
    expect(lastViewHref('u1', servers, dms)).toBeNull()
  })
  it('donne la conversation de la dernière vue', () => {
    saveLastView('u1', '#/s/s1/c9')
    expect(lastViewConv('u1', servers, dms)).toBe('c:c9')
    saveLastView('u1', '#/dm/d1')
    expect(lastViewConv('u1', servers, dms)).toBe('d:d1')
    saveLastView('u1', '#/s/s1') // serveur sans salon ouvert
    expect(lastViewConv('u1', servers, dms)).toBeNull()
  })
})
