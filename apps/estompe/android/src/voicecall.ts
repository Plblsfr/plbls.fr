import type { Platform } from '../../frontend/src/lib/platform'
import { debug } from '../../frontend/src/lib/debuglog.svelte'
import { hrefs } from '../../frontend/src/lib/route.svelte'
import { app } from '../../frontend/src/lib/store.svelte'
import { nativeBridge } from './bridge'

export type VoiceAction = 'mute' | 'deafen' | 'leave' | 'pause' | 'resume'

let pausedByCall = false

export function callLabel(channelId: string): { title: string; href: string } {
  const c = app.channel(channelId)
  return c ? { title: `${c.channel.name} · ${c.server.name}`, href: `/${hrefs.channel(c.server.id, c.channel.id)}` } : { title: 'Salon vocal', href: '/#/' }
}

export async function onVoiceAction(action: VoiceAction) {
  const { voice } = await import('../../frontend/src/blocks/vocal/voice.svelte')
  if (voice.state !== 'connected') return
  if (action === 'mute') await voice.toggleMute()
  else if (action === 'deafen') await voice.toggleDeafen()
  else if (action === 'leave') await voice.leave()
  else if (action === 'pause') {
    debug.log('warn', 'vocal', 'focus audio perdu (appel entrant ou autre application) : micro coupé')
    if (!voice.muted) { pausedByCall = true; await voice.toggleMute() }
  } else if (pausedByCall) {
    pausedByCall = false
    debug.log('info', 'vocal', 'focus audio retrouvé : micro rétabli')
    if (voice.muted && !voice.deafened) await voice.toggleMute()
  }
}

export const voiceCall: NonNullable<Platform['voiceCall']> = {
  update({ channelId, muted, deafened }) {
    const bridge = nativeBridge()
    if (!channelId) { pausedByCall = false; bridge?.voiceStop(); return }
    const { title, href } = callLabel(channelId)
    bridge?.voiceUpdate(title, href, muted, deafened)
  },
}

export function setupVoiceCall() {
  ;(window as unknown as { __estompeVoice: typeof onVoiceAction }).__estompeVoice = onVoiceAction
  document.addEventListener('visibilitychange', () => {
    debug.log('info', 'vocal', document.hidden ? 'application passée en arrière-plan' : 'application revenue au premier plan')
  })
}
