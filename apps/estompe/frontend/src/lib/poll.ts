import type { PollOption } from './types'

export const pollTotal = (options: PollOption[]) => options.reduce((n, o) => n + o.votes, 0)

/** Ids des choix en tête (ex æquo compris) ; aucun tant que personne n'a voté. */
export function pollWinners(options: PollOption[]): string[] {
  const best = Math.max(0, ...options.map((o) => o.votes))
  return best ? options.filter((o) => o.votes === best).map((o) => o.id) : []
}

export function remaining(end: number, now: number) {
  const min = Math.max(1, Math.round((end - now) / 60000))
  return min < 60 ? `${min} min` : min < 1440 ? `${Math.round(min / 60)} h` : `${Math.round(min / 1440)} j`
}
