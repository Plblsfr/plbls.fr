import type { VideoTile } from './voice.svelte'

/** Une tuile de la scène vocale : un participant (avatar) ou l'un de ses flux (caméra, écran). */
export interface StageItem { key: string; userId: string; name: string; tile?: VideoTile }

/** Tous les participants du salon, chacun avec sa caméra (ou son avatar) ; un partage d'écran est une tuile de plus. */
export function buildItems(people: { user_id: string; pseudo: string }[], tiles: VideoTile[]): StageItem[] {
  const out: StageItem[] = []
  const used = new Set<VideoTile>()
  for (const p of people) {
    const mine = tiles.filter((t) => t.userId === p.user_id)
    const cam = mine.find((t) => !t.screen)
    out.push({ key: cam?.sid ?? `u:${p.user_id}`, userId: p.user_id, name: p.pseudo, tile: cam })
    for (const t of mine.filter((t) => t.screen)) out.push({ key: t.sid, userId: p.user_id, name: p.pseudo, tile: t })
    mine.forEach((t) => used.add(t))
  }
  // un flux peut arriver avant la liste des participants : on l'affiche quand même
  for (const t of tiles) if (!used.has(t)) out.push({ key: t.sid, userId: t.userId, name: t.name, tile: t })
  return out
}

/** Colonnes et lignes de la grille pour `n` tuiles ; `narrow` = écran étroit (portrait), une seule colonne jusqu'à 2 tuiles. */
export function gridDims(n: number, narrow = false): { cols: number; rows: number } {
  if (n <= 1) return { cols: 1, rows: 1 }
  const cols = narrow ? 2 : n <= 2 ? 2 : n <= 4 ? 2 : n <= 9 ? 3 : 4
  if (narrow && n === 2) return { cols: 1, rows: 2 }
  return { cols, rows: Math.ceil(n / cols) }
}

/** Faut-il recevoir cette piste ? Le partage d'écran (image et son) n'est reçu que si on le regarde ; `watched` (choix explicite) l'emporte sur le réglage par défaut. `own` : c'est notre propre partage (Alpha), dont on ne rejoue jamais le son. */
export function shouldSubscribe(source: string, watched: boolean | undefined, auto: boolean, own = false): boolean {
  if (own && source === 'screen_share_audio') return false
  return source === 'screen_share' || source === 'screen_share_audio' ? (watched ?? auto) : true
}

/** Spectateurs de chaque flux : `watching` (attribut publié par chaque participant) liste les `sid` des flux qu'il affiche. */
export function collectWatchers(people: { id: string; name: string; watching?: string }[]): Record<string, { id: string; name: string }[]> {
  const out: Record<string, { id: string; name: string }[]> = {}
  for (const p of people) for (const sid of (p.watching ?? '').split(',').filter(Boolean)) (out[sid] ??= []).push({ id: p.id, name: p.name })
  return out
}
