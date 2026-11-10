/** App Android (Tauri mobile) : connexion par schéma d'URL, notifications et push FCM, mise à jour par APK, mode conduite. */
import type { Platform } from '../../../../web/estompe/src/lib/platform'
import { debugStore } from '../../crates/core/ts/tauri'
import { auth } from './auth'
import { nativeBridge, nativeSession } from './bridge'
import { openFromNotification, setupLinks } from './links'
import { notifications } from './notifications'
import SettingsDrive from './SettingsDrive.svelte'
import { setupBgAudio } from './bgaudio.svelte'
import SettingsBgAudio from './SettingsBgAudio.svelte'
import { setupPip } from './pip.svelte'
import SettingsPlayer from './SettingsPlayer.svelte'
import { updates } from './updates'
import { setupVoiceCall, voiceCall } from './voicecall'

export const platform: Platform = {
  id: () => 'android',
  updates,
  auth,
  notifications,
  debugStore,
  screenshot: {
    auto: true,
    async capture() {
      const b64 = (await nativeSession()?.({ op: 'screenshot' }))?.image
      return b64 ? new Blob([Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))], { type: 'image/jpeg' }) : null
    },
  },
  spaces: [],
  slots: { get settingsNotifications() { return nativeBridge()?.driveSettings ? SettingsDrive : undefined }, get settingsVoice() { return nativeBridge()?.pipState ? SettingsPlayer : undefined }, get settingsDebug() { return nativeBridge()?.bgAudioUpdate ? SettingsBgAudio : undefined } },
  voiceCall,
  start() { setupLinks(); setupVoiceCall(); setupPip(); setupBgAudio() },
  windows: {},
  restoresLastView: () => true,
  openFromNotification,
  cancelNotification: (conv) => nativeBridge()?.cancelConv(conv),
  reportShortcut: false,
  micHelp: " Autorisez-le dans les réglages d'Android › Applications › Estompe › Autorisations.",
  label: () => 'app Android',
  pttNote: "La touche ne fonctionne que lorsque l'onglet Estompe est actif et qu'aucun champ de saisie n'a le focus. Pour parler depuis un jeu ou une autre fenêtre, utilise l'application PC.",
}
