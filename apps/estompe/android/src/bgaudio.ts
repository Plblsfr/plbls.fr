export type BgKind = 'watch' | 'music' | 'solo'

export interface BgPlayers { watch: boolean; music: boolean; solo: boolean }

export const bgKind = (p: BgPlayers): BgKind | null => (p.watch ? 'watch' : p.music ? 'music' : p.solo ? 'solo' : null)

export const STALL_MS = 15_000

export interface Sample { pos: number; at: number }

export function stallCheck(prev: Sample | null, pos: number, now: number, watching: boolean): { sample: Sample | null; stalled: boolean } {
  if (!watching) return { sample: null, stalled: false }
  if (!prev || pos !== prev.pos) return { sample: { pos, at: now }, stalled: false }
  return { sample: prev, stalled: now - prev.at >= STALL_MS }
}

export interface BgAudioState { title: string; artist: string; cover: string; playing: boolean; stalled: boolean }

export function bgPayload(on: boolean, kind: BgKind | null, s: BgAudioState) {
  return on && kind ? s : null
}

export function bgRun(action: string, a: { toggle(): void; next(): void }) {
  if (action === 'toggle') a.toggle()
  else if (action === 'next') a.next()
}
