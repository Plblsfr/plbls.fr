import { describe, expect, it } from 'vitest'
import { activityName, listeningLabel, sharedActivity } from './activity.svelte'

describe('activityName', () => {
  const last = { name: 'PokeMMO', at: 1000 }
  it('prend le jeu vu à l\'instant', () => expect(activityName('Factorio', last, 2000)).toBe('Factorio'))
  it('garde le dernier jeu après un alt-tab', () => expect(activityName(null, last, 30_000)).toBe('PokeMMO'))
  it('l\'oublie au bout d\'une minute', () => expect(activityName(null, last, 70_000)).toBe(''))
})

describe('activité d\'écoute', () => {
  it('affiche le morceau écouté seul', () => expect(listeningLabel('Intro')).toBe('🎵 Écoute Intro'))
  it('tient dans les 64 caractères du serveur', () => expect([...listeningLabel('é'.repeat(100))]).toHaveLength(64))
  it('ne montre rien sans consentement', () => expect(sharedActivity('', false, 'Intro', false)).toBe(''))
  it('montre le morceau avec le réglage', () => expect(sharedActivity('', false, 'Intro', true)).toBe('🎵 Écoute Intro'))
  it('le jeu partagé prime sur la musique', () => expect(sharedActivity('Factorio', true, 'Intro', true)).toBe('Factorio'))
  it('un jeu non partagé ne fuit pas', () => expect(sharedActivity('Factorio', false, 'Intro', false)).toBe(''))
})
