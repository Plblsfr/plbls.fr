import { afterEach, describe, expect, it } from 'vitest'
import { platform as web } from '../../web/src/platform'
import { setPlatform } from '../../frontend/src/lib/platform'
import { parseHash } from '../../frontend/src/lib/route.svelte'
import { platform } from './platform'

afterEach(() => setPlatform(web))

describe('plateforme Windows', () => {
  it('ajoute l\'espace Bobyn au routeur, que le web ne connaît pas', () => {
    expect(parseHash('#/bobyn/settings')).toEqual({ name: 'home' })
    setPlatform(platform)
    expect(parseHash('#/bobyn/settings')).toEqual({ name: 'space', space: 'bobyn', section: 'settings' })
  })
  it('déclare les fenêtres d\'overlay et le raccourci global', () => {
    expect(Object.keys(platform.windows)).toEqual(['#overlay', '#bobyn-overlay', '#msg-overlay'])
    expect(platform.globalPtt).toBeDefined()
    expect(web.globalPtt).toBeUndefined()
  })
})
