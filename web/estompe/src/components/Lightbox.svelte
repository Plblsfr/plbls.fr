<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import VideoPlayer from '../blocks/videos/VideoPlayer.svelte'
  import { notify } from '../lib/ui'

  let { src, name, video = false, embed = false, onclose }: { src: string; name: string; video?: boolean; embed?: boolean; onclose: () => void } = $props()

  // Sorti du flux et rattaché au <body> : les parents peuvent avoir un transform qui piègerait position: fixed.
  function portal(node: HTMLElement) {
    document.body.append(node)
    return { destroy: () => node.remove() }
  }

  let box = $state<HTMLElement>()
  onMount(() => {
    const avant = document.activeElement instanceof HTMLElement ? document.activeElement : null
    box?.focus()
    return () => { if (avant?.isConnected) avant.focus() }
  })

  function tab(e: KeyboardEvent) {
    if (e.key !== 'Tab' || !box) return
    const f = [...box.querySelectorAll<HTMLElement>('a[href], button, input, iframe, video[controls], [tabindex]:not([tabindex="-1"])')].filter((el) => !el.hasAttribute('disabled') && !el.closest('[hidden]'))
    if (!f.length) return void e.preventDefault()
    const i = f.indexOf(document.activeElement as HTMLElement)
    const next = e.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : i === f.length - 1 ? 0 : i + 1
    e.preventDefault()
    f[next].focus()
  }

  $effect(() => {
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onclose()
    addEventListener('keydown', key)
    return () => removeEventListener('keydown', key)
  })
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="lightbox" use:portal bind:this={box} role="dialog" aria-modal="true" aria-label={name} tabindex="-1" onkeydown={tab} onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="lightbox-bar">
    <span class="lightbox-icon" aria-hidden="true"><Icon name={embed || video ? 'film' : 'image'} size={16} /></span>
    <span class="lightbox-name">{name}</span>
    {#if !embed}<a class="lightbox-ib" href={src} download={name} aria-label="Télécharger" title="Télécharger" onclick={() => notify.info(`Téléchargement de ${name}…`)}><Icon name="telecharge" size={18} /></a>{/if}
    <button class="lightbox-ib" onclick={onclose} aria-label="Fermer" title="Fermer (Échap)"><Icon name="x" size={18} /></button>
  </div>
  {#if embed}
    <iframe {src} title={name} allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"></iframe>
  {:else if video}
    <VideoPlayer {src} autoplay />
  {:else}
    <img {src} alt={name} />
  {/if}
</div>
