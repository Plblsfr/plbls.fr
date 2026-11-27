<script lang="ts" module>
  import { filmsApi } from '../../lib/api'
  import { afficheBadge, type FilmBadge } from './badges'
  import { paletteDe, type Affiche, type Palette, type Polices } from './badges-anim'

  interface Ressources { img: Affiche | null; palette: Palette }
  const ressources = new Map<string, Promise<Ressources>>()

  async function affiche(b: FilmBadge): Promise<Affiche | null> {
    const a = afficheBadge(b)
    if (!a.image || !filmsApi.base) return null
    try { return await createImageBitmap(await filmsApi.imageBlob(a.id, 'poster', 500)) } catch { return null }
  }

  function pixels(img: Affiche | null): Uint8ClampedArray | null {
    if (!img) return null
    try {
      const c = document.createElement('canvas')
      c.width = 24
      c.height = 36
      const ctx = c.getContext('2d')
      if (!ctx) return null
      ctx.drawImage(img, 0, 0, 24, 36)
      return ctx.getImageData(0, 0, 24, 36).data
    } catch { return null }
  }

  export function ressourcesDe(b: FilmBadge): Promise<Ressources> {
    const cle = afficheBadge(b).id
    let r = ressources.get(cle)
    if (!r) {
      r = affiche(b).then((img) => ({ img, palette: paletteDe(pixels(img), b.title_id) }))
      ressources.set(cle, r)
    }
    return r
  }

  export function polices(): Polices {
    const css = typeof document === 'undefined' ? null : getComputedStyle(document.documentElement)
    const lire = (nom: string, defaut: string) => css?.getPropertyValue(nom).trim() || defaut
    return {
      titre: lire('--font-titre', 'system-ui, sans-serif'),
      texte: lire('--font-texte', 'system-ui, sans-serif'),
      serif: 'Georgia, "DejaVu Serif", serif',
      mono: 'ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace',
    }
  }
</script>

<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { CARTE, DUREES, dessinerBadge, styleDe } from './badges-anim'
  import { etiquetteBadge } from './badges'

  let { badge }: { badge: FilmBadge } = $props()

  let canvas = $state<HTMLCanvasElement | null>(null)
  let pret = $state<Ressources | null>(null)
  let finie = $state(false)
  let jouee = $state(false)
  let debut = 0
  let image = 0

  const style = $derived(styleDe(badge.title_id))
  const calme = () => document.documentElement.dataset.motion === 'reduit' || (typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches)

  function peindre(t: number) {
    const ctx = canvas?.getContext('2d')
    if (!ctx || !pret) return
    ctx.clearRect(0, 0, CARTE.largeur, CARTE.hauteur)
    dessinerBadge(ctx, style, badge, pret.img, pret.palette, polices(), t)
  }

  function jouer(demande = false) {
    cancelAnimationFrame(image)
    if (!pret) return
    if (!demande && calme()) { peindre(DUREES[style]); finie = true; jouee = false; return }
    finie = false
    jouee = true
    debut = performance.now()
    const pas = (now: number) => {
      const t = (now - debut) / 1000
      peindre(t)
      if (t < DUREES[style]) image = requestAnimationFrame(pas)
      else finie = true
    }
    image = requestAnimationFrame(pas)
  }

  $effect(() => {
    let actif = true
    pret = null
    void Promise.all([ressourcesDe(badge), document.fonts?.ready]).then(([r]) => { if (actif) pret = r })
    return () => { actif = false; cancelAnimationFrame(image) }
  })

  $effect(() => {
    if (!pret || !canvas) return
    peindre(0)
    if (typeof IntersectionObserver !== 'function') { jouer(); return }
    let vue = false
    const o = new IntersectionObserver((entrees) => {
      if (vue || !entrees.some((e) => e.isIntersecting)) return
      vue = true
      o.disconnect()
      jouer()
    }, { threshold: 0.5 })
    o.observe(canvas)
    return () => o.disconnect()
  })
</script>

<div class="film-badge-anim" data-style={style}>
  <canvas bind:this={canvas} width={CARTE.largeur} height={CARTE.hauteur} aria-label={`${etiquetteBadge(badge)} : ${badge.title}`}></canvas>
  {#if finie}<button type="button" class="film-badge-rejouer" onclick={() => jouer(true)} aria-label={jouee ? 'Rejouer l’animation' : 'Lire l’animation'}><Icon name={jouee ? 'replay' : 'lecture'} size={16} /></button>{/if}
</div>

<style>
  .film-badge-anim { position: relative; width: 100%; max-width: 360px; aspect-ratio: 1080 / 1350; border-radius: var(--r-l); overflow: hidden; background: #111; box-shadow: var(--o1); }
  .film-badge-anim canvas { display: block; width: 100%; height: 100%; }
  .film-badge-rejouer { position: absolute; top: 8px; right: 8px; display: grid; place-items: center; width: 32px; height: 32px; border: 0; border-radius: 50%; background: rgba(0, 0, 0, 0.55); color: #fff; cursor: pointer; }
  .film-badge-rejouer:focus-visible { outline: none; box-shadow: var(--anneau); }
</style>
