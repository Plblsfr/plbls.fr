import { api } from '../../lib/api'
import type { CustomEmoji } from '../../lib/types'

/** Emojis personnalisés des serveurs de l'utilisateur (:nom:) et leurs images (téléchargées avec l'authentification). */
class Emojis {
  list = $state<CustomEmoji[]>([])
  private urls = $state<Record<string, string>>({})
  private pending = new Set<string>()

  async load() {
    try {
      this.list = await api.emojis()
    } catch { /* la liste reste celle d'avant */ }
  }

  /** Emoji portant ce nom ; celui du serveur courant est préféré en cas d'homonymes. */
  resolve(name: string, serverId?: string): CustomEmoji | undefined {
    const n = name.toLowerCase()
    return this.list.find((e) => e.name === n && e.server_id === serverId) ?? this.list.find((e) => e.name === n)
  }

  names(): string[] {
    return this.list.map((e) => e.name)
  }

  src(id: string): string {
    return this.urls[id] ?? ''
  }

  ensure(id: string) {
    if (this.urls[id] || this.pending.has(id)) return
    this.pending.add(id)
    api.emojiBlobUrl(id).then((u) => { this.urls[id] = u }).catch(() => {}).finally(() => this.pending.delete(id))
  }
}

export const emojis = new Emojis()
