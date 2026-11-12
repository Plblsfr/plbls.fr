/** App Windows (Tauri) : connexion par serveur loopback, toasts, mise à jour par le plugin updater, partage natif, overlays. */
import type { Platform } from '../../../../web/estompe/src/lib/platform'
import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
import { debugStore, invoke } from '../../crates/core/ts/tauri'
import { auth } from './auth'
import { capture, captureSelf } from './capture'
import { startMsgOverlay } from './msg-overlay.svelte'
import MsgOverlay from './MsgOverlay.svelte'
import { notifications } from './notifications'
import Overlay from './Overlay.svelte'
import { overlayUi, startOverlay, toggleOverlay } from './overlay.svelte'
import { globalPtt } from './ptt-global'
import SettingsMesures from './SettingsMesures.svelte'
import SettingsOverlay from './SettingsOverlay.svelte'
import SettingsShare from './SettingsShare.svelte'
import UpdateSettings from './UpdateSettings.svelte'
import { updates } from './updates'

export const platform: Platform = {
  id: () => 'windows',
  updates,
  auth,
  notifications,
  capture,
  globalPtt,
  debugStore,
  screenshot: { auto: true, capture: captureSelf },
  spaces: [],
  slots: { settingsUpdate: UpdateSettings, settingsPtt: SettingsOverlay, settingsShare: SettingsShare, settingsDebug: SettingsMesures },
  start() {
    startOverlay() // overlay des participants du salon vocal
    startMsgOverlay() // messages privés et mentions sur le jeu
  },
  windows: { '#overlay': Overlay, '#msg-overlay': MsgOverlay },
  restoresLastView: () => false,
  openFromNotification: () => false,
  cancelNotification() {},
  reportShortcut: true,
  toH264: async (file) => new Blob([await invoke<ArrayBuffer>('video_to_h264', new Uint8Array(await file.arrayBuffer()))], { type: 'video/mp4' }),
  async foregroundApp() { return (await invoke<{ name: string | null }>('foreground_info')).name },
  micHelp: ' Autorisez-le dans Paramètres Windows › Confidentialité et sécurité › Microphone (applications de bureau).',
  label: () => 'app Windows',
  pttNote: 'La touche fonctionne dans tout Windows, même quand une autre fenêtre a le focus (tant que tu es dans un salon vocal).',
  voiceOverlay: { available: () => prefs.overlayWhen !== 'never', hidden: () => overlayUi.hidden, toggle: toggleOverlay },
}
