import type { WatchPlayer } from '../regarder-ensemble/watch.svelte'

/** Lecteur de la session Music2gether (monté par `MusicDock`) et emplacement où la vue « En cours de lecture » le fait apparaître. */
export const music = $state<{ player: WatchPlayer | null; slot: HTMLElement | null }>({ player: null, slot: null })
