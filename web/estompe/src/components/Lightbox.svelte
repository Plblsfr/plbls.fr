<script lang="ts">
  import Icon from './Icon.svelte'
  import VideoPlayer from '../blocks/videos/VideoPlayer.svelte'
  import { notify } from '../lib/ui'

  let { src, name, video = false, embed = false, onclose }: { src: string; name: string; video?: boolean; embed?: boolean; onclose: () => void } = $props()

  // Sorti du flux et rattaché au <body> : les parents peuvent avoir un transform qui piègerait position: fixed.
  function portal(node: HTMLElement) {
    document.body.append(node)
    return { destroy: () => node.remove() }
  }

  $effect(() => {
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onclose()
    addEventListener('keydown', key)
    return () => removeEventListener('keydown', key)
  })
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="lightbox" use:portal role="dialog" aria-modal="true" aria-label={name} tabindex="-1" onclick={(e) => e.target === e.currentTarget && onclose()}>
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
