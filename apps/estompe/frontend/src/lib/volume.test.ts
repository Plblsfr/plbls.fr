import { describe, expect, it } from 'vitest'
import { gainOf, ownerOf, participantMenu } from './volume'

describe('gainOf', () => {
  const voice = { volumes: { a: 50 }, muted: {} as Record<string, boolean> }
  const screen = { volumes: { a: 200 }, muted: { a: true } as Record<string, boolean> }

  it('applique à chaque piste son propre volume', () => {
    expect(gainOf(false, voice, 'a')).toBe(0.5)
    expect(gainOf(false, { ...screen, muted: {} }, 'a')).toBe(2)
  })
  it('couper le partage laisse la voix', () => {
    expect(gainOf(false, screen, 'a')).toBe(0)
    expect(gainOf(false, voice, 'a')).toBe(0.5)
  })
  it('100 % par défaut, sans effet sur les autres, et la sourdine casque coupe tout', () => {
    expect(gainOf(false, voice, 'b')).toBe(1)
    expect(gainOf(true, voice, 'a')).toBe(0)
  })
})

describe('ownerOf', () => {
  it('rattache le participant du partage natif à son propriétaire', () => {
    expect(ownerOf('u1#screen')).toBe('u1')
    expect(ownerOf('u1')).toBe('u1')
  })
})

describe('participantMenu', () => {
  const base = { self: false, muted: false, canModerate: false, mod: {}, setMuted: () => {}, moderate: () => {} }
  const labels = (items: { label: string }[]) => items.map((i) => i.label)

  it('propose le muet pour soi, sans modération pour un membre ordinaire', () => {
    expect(labels(participantMenu(base))).toEqual(['Rendre muet pour moi'])
    expect(labels(participantMenu({ ...base, muted: true }))).toEqual(['Ne plus rendre muet pour moi'])
  })
  it("n'ajoute rien sur soi-même (menu natif)", () => {
    expect(participantMenu({ ...base, self: true, canModerate: true })).toEqual([])
  })
  it('ajoute le réglage du volume quand la tuile le permet', () => {
    expect(labels(participantMenu({ ...base, adjust: () => {} }))).toEqual(['Rendre muet pour moi', 'Régler le volume'])
  })
  it('ne propose que les coupures permises par les rôles', () => {
    const mod = { ...base, canModerate: true }
    expect(labels(participantMenu({ ...mod, canDeafen: false }))).toEqual(['Rendre muet pour moi', 'Couper le micro du serveur'])
    expect(labels(participantMenu({ ...mod, canMute: false }))).toEqual(['Rendre muet pour moi', 'Couper le son du serveur'])
  })
  it('pour un modérateur, inverse la coupure du micro et du son sans toucher à l\'autre', () => {
    const calls: unknown[] = []
    const items = participantMenu({ ...base, canModerate: true, mod: { mic_off: true }, moderate: (m) => calls.push(m) })
    expect(labels(items)).toEqual(['Rendre muet pour moi', 'Rétablir le micro du serveur', 'Couper le son du serveur'])
    items[1].run()
    items[2].run()
    expect(calls).toEqual([{ mic_off: false, sound_off: false }, { mic_off: true, sound_off: true }])
  })
})
