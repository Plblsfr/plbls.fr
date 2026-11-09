import { api } from '../../lib/api'
import { hrefs } from '../../lib/route.svelte'
import type { SessionKind } from '../../lib/types'
import { voice } from '../vocal/voice.svelte'

/** « Regarder ensemble » (ou Music2gether, `kind` music) dans un salon vocal : on le rejoint au besoin, puis la première vidéo est lue et les suivantes forment la file. */
export async function startWatching(serverId: string, voiceChannelId: string, links: string[], kind: SessionKind = 'watch') {
  if (voice.channelId !== voiceChannelId || voice.state === 'idle') await voice.join(voiceChannelId)
  await api.watchStart(voiceChannelId, links[0], links.slice(1, 50), kind)
  location.hash = hrefs.channel(serverId, voiceChannelId)
}

/** « Rejoindre » une séance en cours : le salon vocal s'ouvre, la session s'y affiche. */
export async function joinSession(serverId: string, voiceChannelId: string) {
  location.hash = hrefs.channel(serverId, voiceChannelId)
  if (voice.channelId !== voiceChannelId || voice.state === 'idle') await voice.join(voiceChannelId)
}
