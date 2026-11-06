import { cairnApi, usesBearer } from '../../lib/api'

/** Action Svelte : en mode Bearer (app Windows), télécharge les images des pages avec l'authentification. `html` ne sert qu'à relancer l'action. */
export function cairnImages(node: HTMLElement, _html?: string) {
  const run = () => {
    if (!usesBearer()) return
    node.querySelectorAll('img').forEach((img) => {
      const m = /\/cairn\/files\/([\w-]+)$/.exec(img.getAttribute('src') ?? '')
      if (!m || img.dataset.loaded) return
      img.dataset.loaded = '1'
      cairnApi.imageBlobUrl(m[1]).then((u) => (img.src = u)).catch(() => {})
    })
  }
  run()
  return { update: run }
}
