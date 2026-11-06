import type { UpdateDriver } from '../../src/lib/platform'

/** Web et PWA : web.json publié avec le site ; l'installation recharge la page après la mise à jour du service worker (le brouillon est déjà enregistré par le composeur). */
export const updates: UpdateDriver = {
  installOnBlock: true,
  installLabel: 'Installer et redémarrer',
  async fetchRemote() {
    const r = await fetch('/updates/web.json', { cache: 'no-store' })
    if (!r.ok) throw new Error(`web.json : HTTP ${r.status}`)
    return r.json()
  },
  async install() {
    const reg = await navigator.serviceWorker?.getRegistration()
    await reg?.update().catch(() => {})
    const sw = reg?.installing ?? reg?.waiting
    if (sw && sw.state !== 'activated') {
      await new Promise<void>((done) => {
        const t = setTimeout(done, 8000)
        sw.addEventListener('statechange', () => { if (sw.state === 'activated') { clearTimeout(t); done() } })
      })
    }
    location.reload()
    return false
  },
}
