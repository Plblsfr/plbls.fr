<script lang="ts">
  import { api } from '../../lib/api'
  import type { LinkPreview } from '../../lib/types'
  import Icon from '../../components/Icon.svelte'
  import Lightbox from '../../components/Lightbox.svelte'
  import { youtubeVideo } from '../regarder-ensemble/youtube'

  let { url }: { url: string } = $props()

  const yt = $derived(youtubeVideo(url))
  // Le lecteur n'est chargé qu'au clic ; une miniature introuvable (vidéo indisponible) laisse le lien simple.
  let playing = $state(false)
  let broken = $state(false)
  let big = $state(false)
  const embedUrl = $derived(yt && `https://www.youtube-nocookie.com/embed/${yt.id}?autoplay=1&start=${yt.start}`)

  // Un aperçu ne se demande qu'une fois par lien et par session (le serveur met aussi en cache).
  const cache = ((globalThis as any).__previewCache ??= new Map<string, Promise<LinkPreview | null>>()) as Map<string, Promise<LinkPreview | null>>
  const preview = $derived.by(() => {
    if (!cache.has(url)) cache.set(url, api.linkPreview(url).catch(() => null))
    return cache.get(url)!
  })
</script>

{#if yt}
  {#if !broken}
    <!-- Titre et description via l'aperçu serveur (comme les autres liens) ; sans lui, la miniature seule. -->
    {#await preview then p}
      {#if p}
        <div class="preview yt-info">
          <span>
            {#if p.site_name}<span class="muted small">{p.site_name}</span>{/if}
            <a href={p.url} target="_blank" rel="noopener noreferrer nofollow"><strong>{p.title}</strong></a>
            {#if p.description}<span class="muted small yt-desc">{p.description}</span>{/if}
          </span>
        </div>
      {/if}
    {/await}
    <div class="yt">
      {#if playing}
        <iframe src={embedUrl} title="Lecteur YouTube" allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"></iframe>
      {:else}
        <button type="button" onclick={() => (playing = true)} aria-label="Lire la vidéo YouTube">
          <img src="https://img.youtube.com/vi/{yt.id}/hqdefault.jpg" alt="" loading="lazy" referrerpolicy="no-referrer" onerror={() => (broken = true)} />
          <span class="play" aria-hidden="true">▶</span>
        </button>
      {/if}
      <button type="button" class="yt-big" onclick={() => { playing = false; big = true }} aria-label="Agrandir la vidéo"><Icon name="expand" size={16} /></button>
    </div>
    {#if big}<Lightbox src={embedUrl!} name="Vidéo YouTube" embed onclose={() => (big = false)} />{/if}
  {/if}
{:else}
{#await preview then p}
  {#if p}
    <a class="preview" href={p.url} target="_blank" rel="noopener noreferrer nofollow">
      <span>
        {#if p.site_name}<span class="muted small">{p.site_name}</span>{/if}
        <strong>{p.title}</strong>
        {#if p.description}<span class="muted small">{p.description}</span>{/if}
      </span>
      {#if p.image}<img src={p.image} alt="" loading="lazy" referrerpolicy="no-referrer" />{/if}
    </a>
  {/if}
{/await}
{/if}
