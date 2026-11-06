// Partage natif ALPHA (Rust, Windows) : liste des écrans et fenêtres avec miniatures, puis capture et publication par Rust.
// Le webview ne sait pas désigner une source précise (getDisplayMedia affiche son propre sélecteur).
import type { CaptureDriver } from '../../../../web/estompe/src/lib/platform'
import { invoke } from '../../crates/core/ts/tauri'

/** Partage natif (EST-18) : Rust capture la source et la publie lui-même dans le salon (participant `#screen`), sans repasser par le webview.
 *  `start` renvoie les encodeurs vidéo disponibles côté Rust. `audio` (EST-52) : publie aussi le son (de la fenêtre, ou du système sans Estompe). */
export const capture: CaptureDriver = {
  listSources: () => invoke('capture_sources'),
  async thumbUrl(id) {
    const buf = await invoke<ArrayBuffer>('capture_thumb', { id })
    return URL.createObjectURL(new Blob([buf], { type: 'image/jpeg' }))
  },
  start: (url, token, id, fps, height, bitrate, audio) => invoke<string>('screen_native_start', { url, token, id, fps, height, bitrate, audio }),
  stop: () => invoke('screen_native_stop'),
}

/** Capture de la fenêtre d'Estompe pour un signalement (EST-105). */
export const captureSelf = async () => new Blob([await invoke<ArrayBuffer>('capture_self')], { type: 'image/jpeg' })
