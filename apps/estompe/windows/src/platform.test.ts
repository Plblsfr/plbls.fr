import { afterEach, describe, expect, it } from 'vitest'
import { platform as web } from '../../../../web/estompe/plateforme/src/platform'
import { setPlatform } from '../../../../web/estompe/src/lib/platform'
import { platform } from './platform'

afterEach(() => setPlatform(web))

describe('plateforme Windows', () => {
  it('déclare les fenêtres d\'overlay et le raccourci global', () => {
    expect(Object.keys(platform.windows)).toEqual(['#overlay', '#msg-overlay'])
    expect(platform.globalPtt).toBeDefined()
    expect(web.globalPtt).toBeUndefined()
  })
})
