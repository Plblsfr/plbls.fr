<script lang="ts">
  import Icon from './Icon.svelte'
  import { notify } from '../lib/ui'
  import Lightbox from './Lightbox.svelte'
  import VideoPlayer from '../blocks/videos/VideoPlayer.svelte'
  import { api, usesBearer } from '../lib/api'
  import { formatDuration, formatSize } from '../lib/format'
  import { gameOfName } from '../lib/format'
  import type { Attachment } from '../lib/types'

  let { attachment: a }: { attachment: Attachment } = $props()

  const isImage = $derived(['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(a.mime))
  const isVideo = $derived(['video/mp4', 'video/webm'].includes(a.mime) && a.kind !== 'video')
  const isReplay = $derived(a.kind === 'video' && !a.expired)
  const direct = $derived(api.fileUrl(a.id))
  let open = $state(false)

  // En mode Bearer (app Windows), ni <img> ni <a> ne peuvent envoyer l'en-tête : téléchargement puis URL locale.
  let blobUrl = $state('')
  let failed = $state(false)
  async function load() {
    try { blobUrl = await api.fileBlobUrl(a.id) } catch { failed = true }
  }
  $effect(() => {
    if (usesBearer() && !a.expired && !isReplay && (isImage || isVideo || !blobUrl)) void load()
  })
  const src = $derived(usesBearer() ? blobUrl : direct)

  // Une vidéo de replay se lit en streaming (requêtes Range) : <video> et <img> n'envoient pas l'en-tête Bearer, d'où un lien signé.
  let token = $state('')
  let playing = $state(false)
  let unplayable = $state(false)
  let retried = $state(false)
  function onVideoError() {
    if (usesBearer() && !retried) { retried = true; void refreshToken() } else unplayable = true
  }
  const signed = (path: string) => (usesBearer() ? (token ? `${path}?t=${token}` : '') : path)
  const videoSrc = $derived(signed(direct))
  const thumbSrc = $derived(a.thumb ? signed(api.thumbUrl(a.id)) : '')
  const downloadSrc = $derived(signed(direct) && (usesBearer() ? `${signed(direct)}&download=1` : `${direct}?download=1`))
  async function refreshToken() {
    try { token = await api.fileToken(a.id) } catch { failed = true }
  }
  $effect(() => {
    if (isReplay && usesBearer() && !token) void refreshToken()
  })
</script>

{#if a.expired}
  <span class="file-chip expired" title="Supprimé du serveur pour faire de la place"><Icon name="file" size={16} /> {a.name} <span class="small">(fichier expiré)</span></span>
{:else if isReplay}
  <div class="replay-att">
    {#if playing && videoSrc}
      <VideoPlayer src={videoSrc} poster={thumbSrc || undefined} autoplay onerror={onVideoError} />
      {#if unplayable}<p class="ra-error small">Cette vidéo ne peut pas être lue ici (codec non pris en charge ou fichier illisible). Utilisez le bouton de téléchargement.</p>{/if}
    {:else}
      <button class="ra-poster" onclick={() => (playing = true)} aria-label="Lire {a.name}" disabled={!videoSrc}>
        {#if thumbSrc}<img src={thumbSrc} alt="" loading="lazy" />{/if}
        <span class="ra-play"><Icon name="play" size={28} /></span>
        {#if a.duration_ms}<span class="ra-badge">{formatDuration(a.duration_ms / 1000)}</span>{/if}
      </button>
    {/if}
    <div class="ra-meta">
      <span class="truncate" title={a.name}>{gameOfName(a.name) || a.name}</span>
      <span class="muted small">{formatSize(a.size)}</span>
      <button class="ib" onclick={() => (open = true)} aria-label="Agrandir la vidéo" title="Agrandir" disabled={!videoSrc}><Icon name="expand" size={16} /></button>
      <a class="ib" href={downloadSrc || undefined} download={a.name} aria-label="Télécharger" title="Télécharger" onclick={() => notify.info(`Téléchargement de ${a.name}…`)}><Icon name="down" size={16} /></a>
    </div>
    {#if open}<Lightbox src={videoSrc} name={a.name} video onclose={() => (open = false)} />{/if}
  </div>
{:else if isImage}
  {#if src}
    <a href={src} onclick={(e) => { e.preventDefault(); open = true }}><img {src} alt={a.name} loading="lazy" /></a>
    {#if open}<Lightbox {src} name={a.name} onclose={() => (open = false)} />{/if}
  {:else if failed}
    <span class="file-chip expired"><Icon name="image" size={16} /> {a.name} (indisponible)</span>
  {/if}
{:else if isVideo}
  {#if src}
    <div class="video-att">
      <VideoPlayer {src} onerror={() => (unplayable = true)} />
      {#if unplayable}<a class="file-chip" href={src} download={a.name}><Icon name="down" size={16} /> Lecture impossible : télécharger {a.name}</a>{/if}
    </div>
  {:else if failed}
    <span class="file-chip expired"><Icon name="file" size={16} /> {a.name} (indisponible)</span>
  {/if}
{:else}
  <a class="file-chip" href={src || direct} download={a.name} target="_blank" rel="noopener noreferrer" title="Télécharger" onclick={() => notify.info(`Téléchargement de ${a.name}…`)}>
    <Icon name="down" size={16} /> <span class="truncate">{a.name}</span> <span class="muted small">{formatSize(a.size)}</span>
  </a>
{/if}
