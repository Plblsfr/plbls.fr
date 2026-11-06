type Box = { left: number; top: number; right: number; bottom: number }

// Place une bulle de taille w×h près de `a` (au-dessus si la place le permet, sinon en dessous), toujours dans la fenêtre visible.
export function placePopup(a: Box, w: number, h: number, vw: number, vh: number, marge = 8) {
  const left = Math.max(marge, Math.min(a.left, vw - w - marge))
  const top = a.top - h >= marge ? a.top - h : Math.min(a.bottom, Math.max(marge, vh - h - marge))
  return { left, top }
}

// Svelte action : sort la bulle du flux (position fixe, donc jamais coupée par un conteneur à défilement).
export function popupAt(node: HTMLElement) {
  const anchor = node.parentElement!.getBoundingClientRect()
  const { left, top } = placePopup(anchor, node.offsetWidth, node.offsetHeight, window.innerWidth, window.innerHeight)
  node.style.position = 'fixed'
  node.style.left = `${left}px`
  node.style.top = `${top}px`
  node.style.bottom = 'auto'
}
