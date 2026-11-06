import { beforeEach, describe, expect, it, vi } from 'vitest'
import { app } from '../../frontend/src/lib/store.svelte'
import { callLabel, onVoiceAction, voiceCall } from './voicecall'
import { voice } from '../../frontend/src/blocks/vocal/voice.svelte'

describe('voiceCall', () => {
  const bridge = { voiceUpdate: vi.fn(), voiceStop: vi.fn() }
  beforeEach(() => {
    bridge.voiceUpdate.mockReset()
    bridge.voiceStop.mockReset()
    vi.stubGlobal('window', { EstompeNative: bridge })
  })

  it('libellé : salon, serveur et lien du salon', () => {
    app.details = { s1: { server: { id: 's1', name: 'Maison' }, channels: [{ id: 'c1', name: 'général' }] } } as never
    expect(callLabel('c1')).toEqual({ title: 'général · Maison', href: '/#/s/s1/c1' })
    expect(callLabel('inconnu').href).toBe('/#/')
  })

  it('met à jour la notification en vocal et arrête le service à la sortie', () => {
    voiceCall.update({ channelId: 'c1', muted: true, deafened: false })
    expect(bridge.voiceUpdate).toHaveBeenCalledWith('général · Maison', '/#/s/s1/c1', true, false)
    voiceCall.update({ channelId: null, muted: false, deafened: false })
    expect(bridge.voiceStop).toHaveBeenCalled()
  })

  it('appel entrant : coupe le micro puis le rétablit, sans toucher un micro déjà coupé', async () => {
    const toggle = vi.spyOn(voice, 'toggleMute').mockImplementation(async () => { voice.muted = !voice.muted })
    voice.state = 'connected'
    voice.muted = false
    await onVoiceAction('pause')
    expect(voice.muted).toBe(true)
    await onVoiceAction('resume')
    expect(voice.muted).toBe(false)
    voice.muted = true
    toggle.mockClear()
    await onVoiceAction('pause')
    await onVoiceAction('resume')
    expect(toggle).not.toHaveBeenCalled()
    voice.state = 'idle'
  })
})
