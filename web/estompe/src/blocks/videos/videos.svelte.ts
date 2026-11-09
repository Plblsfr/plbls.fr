import { api } from '../../lib/api'
import type { VideosBoard } from '../../lib/types'

/** Contenu des salons « Vidéos » ouverts (publications, listes, séances), rechargé à chaque évènement `videos.changed`. */
class Videos {
  boards = $state<Record<string, VideosBoard>>({})
  private loading = new Map<string, Promise<void>>()
  private again = new Set<string>()

  load(cid: string): Promise<void> {
    const cur = this.loading.get(cid)
    if (cur) { this.again.add(cid); return cur } // un évènement est arrivé pendant le chargement : on recharge ensuite
    const p = api.videos(cid).then((b) => { this.boards[cid] = b }).catch(() => {}).finally(() => {
      this.loading.delete(cid)
      if (this.again.delete(cid)) void this.load(cid)
    })
    this.loading.set(cid, p)
    return p
  }

  /** Évènement reçu : recharge seulement un salon déjà chargé. */
  changed(cid: string) { if (cid in this.boards) void this.load(cid) }

  /** Réaction reçue : met à jour le compte de la carte (le 👍 de la personne aussi). */
  reaction(messageId: string, emoji: string, added: boolean, mine: boolean) {
    for (const b of Object.values(this.boards)) {
      const p = b.posts.find((p) => p.message_id === messageId)
      if (!p) continue
      p.reactions = Math.max(0, p.reactions + (added ? 1 : -1))
      if (mine && emoji === '👍') p.liked = added
    }
  }

  drop(cid: string) { delete this.boards[cid] }
}
export const videos = new Videos()
