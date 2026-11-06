import { api } from '../../lib/api'
import { isTrackRef, musicLink, artistOf, shuffled, videoTitle } from './videos'
import type { VideoPost } from '../../lib/types'

/** Un morceau de la file d'écoute personnelle. `cover` : chemin de l'API (`/music/cover/{id}`), vide si inconnu. */
export interface SoloTrack { id: string; title: string; artist: string; cover: string }

const MAX_QUEUE = 200

/** Morceau d'une carte (hors playlist, qu'il faut d'abord résoudre côté serveur). */
export const trackOfPost = (p: Pick<VideoPost, 'ref' | 'site' | 'title' | 'url' | 'provider'>): SoloTrack => ({ id: p.ref, title: videoTitle(p), artist: artistOf(p), cover: `/music/cover/${p.ref}` })

/**
 * Écoute personnelle : file, morceau en cours, volume propre. Aucune synchronisation, aucun salon vocal concerné ;
 * le lecteur officiel YouTube est monté par `SoloDock`.
 */
class Solo {
  queue = $state<SoloTrack[]>([])
  index = $state(-1)
  playing = $state(false)
  volume = $state(70)
  error = $state('')
  /** Position et durée (s) du morceau en cours, relevées sur le lecteur par `SoloDock`. */
  position = $state(0)
  duration = $state(0)
  /** Déplacement dans le morceau, branché par `SoloDock` sur le lecteur. */
  seekTo: ((s: number) => void) | null = null
  /** Lecture automatique refusée par le système : il faut un geste de la personne pour avoir le son. */
  blocked = $state(false)

  get current(): SoloTrack | undefined { return this.queue[this.index] }

  /** Lit ces cartes (les playlists sont développées par le serveur), dans l'ordre ou mélangées. */
  async play(posts: VideoPost[], opts: { shuffle?: boolean; append?: boolean } = {}) {
    this.error = ''
    try {
      const lists = await Promise.all(posts.map(async (p) => (isTrackRef(p.ref) ? [trackOfPost(p)] : (await api.musicResolve(musicLink(p.ref))).tracks.map(fromApi))))
      let tracks = lists.flat().slice(0, MAX_QUEUE)
      if (opts.shuffle) tracks = shuffled(tracks)
      if (!tracks.length) return
      if (opts.append && this.current) {
        this.queue = [...this.queue, ...tracks].slice(0, MAX_QUEUE)
        return
      }
      this.queue = tracks
      this.index = 0
      this.playing = true
    } catch (e) {
      this.error = e instanceof Error ? e.message : 'Lecture impossible'
      throw e
    }
  }

  next() {
    if (this.index + 1 < this.queue.length) this.index++
    else this.stop()
  }

  seek(s: number) { this.position = s; this.seekTo?.(s) }

  toggle() { if (this.current) this.playing = !this.playing }

  stop() { this.queue = []; this.index = -1; this.playing = false }

  /** Titres encore à venir (après le morceau en cours). */
  get upcoming(): SoloTrack[] { return this.queue.slice(this.index + 1) }
}

const fromApi = (t: { id: string; title: string; artist: string; cover: string }): SoloTrack => ({ id: t.id, title: t.title, artist: t.artist, cover: t.cover })

export const solo = new Solo()
