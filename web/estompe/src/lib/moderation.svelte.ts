import { api } from './api'
import { app } from './store.svelte'
import { notify } from './ui'
import { isTimedOut, outranks } from './roles'
import type { ServerDetail } from './types'

export type ModKind = 'kick' | 'ban' | 'timeout' | 'warn' | 'nickname'

/** Action de modération en cours de confirmation (modale ouverte par le menu d'un membre ou son profil). */
export const moderation = $state<{ at: { kind: ModKind; serverId: string; userId: string; pseudo: string } | null }>({ at: null })

export const askModeration = (kind: ModKind, serverId: string, userId: string, pseudo: string) => (moderation.at = { kind, serverId, userId, pseudo })

export interface ModAction { label: string; danger?: boolean; run: () => void }

/** Actions de modération qu'on peut exercer sur un membre : la permission correspondante et un rang strictement supérieur au sien. */
export function moderationActions(detail: ServerDetail | undefined, userId: string): ModAction[] {
  if (!detail || !outranks(detail, app.meId, userId)) return []
  const sid = detail.server.id
  const m = detail.members.find((m) => m.id === userId)
  const pseudo = m?.pseudo ?? '?'
  const out: ModAction[] = []
  if (app.can(sid, 'timeout_members')) {
    if (isTimedOut(m)) {
      out.push({ label: 'Lever l\'exclusion temporaire', run: () => void api.untimeoutMember(sid, userId).then(() => notify.ok('Exclusion levée'), (e) => notify.error(e, 'Action impossible')) })
    } else {
      out.push({ label: 'Exclure temporairement…', run: () => askModeration('timeout', sid, userId, pseudo) })
    }
    out.push({ label: 'Avertir…', run: () => askModeration('warn', sid, userId, pseudo) })
  }
  if (app.can(sid, 'manage_nicknames')) out.push({ label: 'Changer le pseudo…', run: () => askModeration('nickname', sid, userId, pseudo) })
  if (app.can(sid, 'kick_members')) out.push({ label: 'Expulser…', danger: true, run: () => askModeration('kick', sid, userId, pseudo) })
  if (app.can(sid, 'ban_members')) out.push({ label: 'Bannir…', danger: true, run: () => askModeration('ban', sid, userId, pseudo) })
  return out
}

/** Fenêtre de modération d'un salon texte (mode lent, verrouillage, suppression en masse). */
export const channelMod = $state<{ at: { serverId: string; channelId: string } | null }>({ at: null })
