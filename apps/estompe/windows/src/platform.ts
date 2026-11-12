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
import { demarrerMontage } from './montage.svelte'
import { globalPtt } from './ptt-global'
import { ESPACE, PAGE_REGLAGES } from './replay'
import { startReplayOverlay } from './replay-overlay.svelte'
import { demarrerReplay, replay } from './replay.svelte'
import ReplayGalerie from './ReplayGalerie.svelte'
import ReplayNav from './ReplayNav.svelte'
import ReplayOverlay from './ReplayOverlay.svelte'
import SettingsMesures from './SettingsMesures.svelte'
import SettingsOverlay from './SettingsOverlay.svelte'
import SettingsReplay from './SettingsReplay.svelte'
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
  spaces: [{ id: ESPACE, title: 'Clips', icon: 'clap', visible: () => replay.present && (replay.reglages.actif || replay.clips.length > 0), nav: ReplayNav, page: ReplayGalerie }],
  slots: { settingsUpdate: UpdateSettings, settingsPtt: SettingsOverlay, settingsShare: SettingsShare, settingsDebug: SettingsMesures,
    settingsPages: [{
      id: PAGE_REGLAGES, label: 'Instant replay', icon: 'replay', badge: 'bêta', visible: () => replay.present, page: SettingsReplay,
      index: ['Clips', 'Tampon', 'Résolution', 'Images par seconde', 'Durée gardée', 'Qualité', 'Codec', 'Enregistrer mon micro', 'Afficher le curseur', 'Raccourci de sauvegarde', 'Jeux ajoutés', 'Jeux exclus', 'Dossier des clips', 'Quota'],
    }],
  },
  start() {
    startOverlay() // overlay des participants du salon vocal
    startMsgOverlay() // messages privés et mentions sur le jeu
    void demarrerReplay().then(demarrerMontage)
    startReplayOverlay()
  },
  windows: { '#overlay': Overlay, '#msg-overlay': MsgOverlay, '#replay-overlay': ReplayOverlay },
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
