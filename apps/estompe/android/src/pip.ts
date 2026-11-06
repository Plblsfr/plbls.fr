export type PipKind = 'watch' | 'music' | 'solo'
export type PipEvent = 'toggle' | 'next' | 'close' | 'enter' | 'leave' | 'unavailable'

export interface PipPlayers { watch: boolean; music: boolean; solo: boolean }

export const pipKind = (p: PipPlayers): PipKind | null => (p.watch ? 'watch' : p.music ? 'music' : p.solo ? 'solo' : null)

/** État envoyé au natif : `square` pour la musique et l'écoute seul, `playing` pour l'icône de lecture ou pause. */
export function pipState(floating: boolean, kind: PipKind | null, playing: boolean) {
  return { active: floating && kind !== null, square: kind !== 'watch', playing }
}

export interface PipActions { toggle(): void; next(): void; close(): void; pause(): void }

/** Évènement du natif sur la fenêtre flottante ; renvoie vrai si la lecture a été mise en pause faute de fenêtre. */
export function pipRun(evt: PipEvent, a: PipActions): boolean {
  if (evt === 'toggle') a.toggle()
  else if (evt === 'next') a.next()
  else if (evt === 'close') a.close()
  else if (evt === 'unavailable') { a.pause(); return true }
  return false
}
