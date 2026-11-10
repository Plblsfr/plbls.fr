import { nativeBridge } from './bridge'

const APP_ORIGIN = 'https://estompe.plbls.fr'

/** Lien d'un message dans l'app : `hash` (vue de l'app), `open` (navigateur ou application système), null (refusé). */
export function linkAction(href: string): { hash: string } | { open: string } | null {
  let u: URL
  try { u = new URL(href) } catch { return null }
  if ((u.origin === APP_ORIGIN || u.origin === location.origin) && u.hash.startsWith('#/')) return { hash: u.hash }
  return ['https:', 'mailto:', 'tel:'].includes(u.protocol) ? { open: u.href } : null
}

/** La webview ignore target="_blank" : on ouvre les liens des messages nous-mêmes. */
export function setupLinks() {
  document.addEventListener('click', (e) => {
    const a = (e.target as Element | null)?.closest?.('a[href]')
    if (!a || e.defaultPrevented) return
    e.preventDefault()
    const act = linkAction((a as HTMLAnchorElement).href)
    if (!act) return
    if ('hash' in act) location.hash = act.hash.slice(1)
    else import('@tauri-apps/plugin-opener').then(({ openUrl }) => openUrl(act.open))
  })
}

/** Ouvre la conversation de la notification qui a lancé l'app. */
export function openFromNotification(): boolean {
  const url = nativeBridge()?.takeOpenUrl()
  if (!url?.startsWith('/#/')) return false
  location.hash = url.slice(1)
  return true
}
