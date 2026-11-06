import { describe, expect, it } from 'vitest'
import { audioConfig, audioRows, bobynHotkey, micFromPrefs } from './bobyn'

describe('sources audio de Bobyn', () => {
  it('liste le jeu, le système, les applications et les sources absentes', () => {
    const rows = audioRows([{ source: 'game', gain: 0.5 }, { source: 'app:old.exe', gain: 1 }, { source: 'mic', gain: 1 }], ['discord.exe'])
    expect(rows.map((r) => [r.source, r.on, r.gain])).toEqual([
      ['game', true, 0.5], ['voice', false, 1], ['system', false, 1], ['system-novoice', false, 1], ['app:discord.exe', false, 1], ['app:old.exe', true, 1],
    ])
    expect(rows[5].label).toBe('old.exe')
  })

  it('reconstruit la configuration avec le micro en dernier', () => {
    const rows = audioRows([{ source: 'system', gain: 2 }], [])
    expect(audioConfig(rows, '', '')).toEqual([{ source: 'system', gain: 2 }])
    expect(audioConfig(rows, 'default', ' V ')).toEqual([{ source: 'system', gain: 2 }, { source: 'mic', gain: 1, device: null, ptt: 'V' }])
    expect(audioConfig(rows, 'id1', '')[1]).toMatchObject({ device: 'id1', ptt: null })
  })
})

describe('raccourci de Bobyn', () => {
  const k = (code: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}) =>
    bobynHotkey({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods })

  it('compose modificateurs et touche', () => {
    expect(k('F10', { altKey: true })).toBe('Alt+F10')
    expect(k('KeyS', { ctrlKey: true, shiftKey: true })).toBe('Ctrl+Shift+S')
    expect(k('Digit5', { metaKey: true })).toBe('Win+5')
    expect(k('F9')).toBe('F9')
  })

  it('ignore les modificateurs seuls et les touches non gérées', () => {
    expect(k('AltLeft', { altKey: true })).toBe('')
    expect(k('F25')).toBe('')
    expect(k('Space')).toBe('')
  })
})

describe('micro des Paramètres', () => {
  const mics = [{ id: 'a', name: 'Microphone (USB Audio)' }, { id: 'b', name: 'Casque (Realtek)' }]
  it('rapproche le libellé WebRTC du nom WASAPI', () => {
    expect(micFromPrefs('Par défaut - Casque (Realtek)', mics)).toBe('b')
    expect(micFromPrefs('microphone (usb audio)', mics)).toBe('a')
    expect(micFromPrefs('', mics)).toBeNull()
    expect(micFromPrefs('Inconnu', mics)).toBeNull()
  })
})
