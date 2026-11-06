import type { Update } from '@tauri-apps/plugin-updater'
import type { Remote } from '../../frontend/src/lib/updater.svelte'
import type { UpdateDriver } from '../../frontend/src/lib/platform'

let pending: Update | null = null

/** Plugin updater de Tauri (latest.json signé) ; l'installeur ferme puis relance l'application. */
export const updates: UpdateDriver = {
  installOnBlock: false,
  installLabel: 'Installer et redémarrer',
  version: async () => (await import('@tauri-apps/api/app')).getVersion(),
  async fetchRemote() {
    const { check } = await import('@tauri-apps/plugin-updater')
    pending = await check()
    return pending ? { ...pending.rawJson, version: pending.version, notes: pending.body ?? '' } as Remote : null
  },
  async install(_, progress) {
    if (!pending) throw new Error('aucune mise à jour à installer')
    let total = 0
    let done = 0
    await pending.downloadAndInstall((e) => {
      if (e.event === 'Started') total = e.data.contentLength ?? 0
      else if (e.event === 'Progress') { done += e.data.chunkLength; progress(total ? done / total : 0) }
    })
    return false
  },
}
