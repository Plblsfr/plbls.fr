import { describe, it, expect } from 'vitest'
import { router } from '../../../../web/estompe/src/lib/route.svelte'
import { onToastAction } from './notifications'

describe('onToastAction', () => {
  it('« read » marque la conversation lue sans naviguer', () => {
    const read: string[] = []
    const before = router.current
    onToastAction({ action: 'read', href: '/#/dm/d1', conv: 'd:d1' }, (c) => read.push(c))
    expect(read).toEqual(['d:d1'])
    expect(router.current).toBe(before)
  })
  it('clic ou « Répondre » ouvre la conversation', () => {
    const read: string[] = []
    onToastAction({ action: 'open', href: '/#/dm/d1', conv: 'd:d1' }, (c) => read.push(c))
    expect(read).toEqual([])
    expect(location.hash).toContain('d1')
  })
})
