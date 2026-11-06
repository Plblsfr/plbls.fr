export function startViewport(): void {
  const vv = window.visualViewport
  if (!vv) return
  const sync = () => {
    document.documentElement.style.setProperty('--vvh', `${vv.height}px`)
    if (window.scrollY || window.scrollX) window.scrollTo(0, 0)
  }
  vv.addEventListener('resize', sync)
  sync()
}
