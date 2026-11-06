import type { UpdateDriver } from '../../frontend/src/lib/platform'
import { nativeFetch } from '../../crates/core/ts/tauri'
import { nativeBridge } from './bridge'

const UPDATES = 'https://estompe.plbls.fr/updates'

/** Pas de mise à jour intégrée : android.json donne la dernière version, l'APK est téléchargé, sa somme vérifiée, puis l'installeur du système s'ouvre (l'app est remplacée une fois l'installation validée). */
export const updates: UpdateDriver = {
  installOnBlock: false,
  installLabel: 'Installer',
  async fetchRemote() {
    const r = await nativeFetch(`${UPDATES}/android.json`)
    if (!r.ok) throw new Error(`android.json : HTTP ${r.status}`)
    return r.json()
  },
  async install(available, progress) {
    const native = nativeBridge()!
    native.installApk(`${UPDATES}/Estompe.apk`, available?.sha256 ?? '')
    let state = 'busy'
    while ((state = native.installState()) === 'busy') {
      progress(native.installProgress())
      await new Promise((r) => setTimeout(r, 500))
    }
    if (state !== 'done') throw state // affiché tel quel dans les réglages
    return true
  },
}
