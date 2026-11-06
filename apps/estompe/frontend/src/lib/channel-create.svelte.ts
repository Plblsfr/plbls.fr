import type { Channel, ChannelKind } from './types'

export const MAX_CHANNEL_NAME = 30
export const MAX_TOPIC = 200
export const SLOW: [string, number][] = [['Désactivé', 0], ['5 secondes', 5], ['10 secondes', 10], ['30 secondes', 30], ['1 minute', 60], ['5 minutes', 300], ['15 minutes', 900], ['1 heure', 3600], ['6 heures', 21600]]
export const channelCreate = $state<{
  at: { serverId: string; kind: ChannelKind; after?: string } | null
  fresh: string
  done: { serverId: string; channel: Channel } | null
}>({ at: null, fresh: '', done: null })

export const openNewChannel = (serverId: string, kind: ChannelKind = 'text', after?: string) => (channelCreate.at = { serverId, kind, after })
export function channelNameError(name: string, kind: ChannelKind, channels: Pick<Channel, 'name' | 'kind'>[]): string {
  const n = name.trim()
  if (!n) return 'Donne un nom au salon.'
  if ([...n].length > MAX_CHANNEL_NAME) return `${MAX_CHANNEL_NAME} caractères maximum.`
  if (/\p{Cc}/u.test(n)) return 'Ce nom contient un caractère non autorisé.'
  if (channels.some((c) => c.kind === kind && c.name.toLowerCase() === n.toLowerCase())) return 'Un salon de ce type porte déjà ce nom.'
  return ''
}
