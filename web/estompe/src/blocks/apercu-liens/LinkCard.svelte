<script lang="ts" module>
  import { Lru } from '../../lib/lru'
  import { hrefs } from '../../lib/route.svelte'
  import type { LinkPreview, SillonCard } from '../../lib/types'

  const cache = new Lru<string, Promise<LinkPreview | null>>(200)
  const GENRES: Record<SillonCard['type'], string> = { album: 'Album', track: 'Titre', artist: 'Artiste' }
  const ecouter = (s: SillonCard) => (s.type === 'artist' ? hrefs.musique('artist', s.id) : hrefs.musique('album', s.album ?? s.id))
</script>

<script lang="ts">
  import { api } from '../../lib/api'
  import Icon from '../../components/Icon.svelte'
  import Lightbox from '../../components/Lightbox.svelte'
  import { youtubeVideo } from '../regarder-ensemble/youtube'
  import SillonCover from '../../sections/sillon/SillonCover.svelte'

  let { url, message }: { url: string; message?: string } = $props()

  const yt = $derived(youtubeVideo(url))
  // Le lecteur n'est chargé qu'au clic ; une miniature introuvable (vidéo indisponible) laisse le lien simple.
  let playing = $state(false)
  let broken = $state(false)
  let big = $state(false)
  const embedUrl = $derived(yt && `https://www.youtube-nocookie.com/embed/${yt.id}?autoplay=1&start=${yt.start}`)

  // Un aperçu ne se demande qu'une fois par lien et par message parmi les 200 derniers affichés (le serveur met aussi en cache) :
  // une carte Sillon dépend de l'auteur du message.
  const preview = $derived(cache.getOrSet(`${message ?? ''} ${url}`, () => api.linkPreview(url, message).catch(() => null)))
</script>

{#if yt}
  {#if !broken}
    <!-- Titre et description via l'aperçu serveur (comme les autres liens) ; sans lui, la miniature seule. -->
    {#await preview then p}
      {#if p}
        <div class="lien-apercu yt-info">
          <span>
            {#if p.site_name}<span class="lien-site">{p.site_name}</span>{/if}
            <a href={p.url} target="_blank" rel="noopener noreferrer nofollow"><strong>{p.title}</strong></a>
            {#if p.description}<span class="lien-desc yt-desc">{p.description}</span>{/if}
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
          <span class="play" aria-hidden="true"><Icon name="lecture" size={22} /></span>
        </button>
      {/if}
      <button type="button" class="yt-big" onclick={() => { playing = false; big = true }} aria-label="Agrandir la vidéo"><Icon name="agrandir" size={16} /></button>
    </div>
    {#if big}<Lightbox src={embedUrl!} name="Vidéo YouTube" embed onclose={() => (big = false)} />{/if}
  {/if}
{:else}
{#await preview then p}
  {#if p?.sillon}
    {@const s = p.sillon}
    <div class="lien-apercu sillon-carte">
      <span class="sillon-carte-pochette"><SillonCover id={s.cover?.id ?? s.id} title={s.title} image={!!s.cover} kind={s.cover?.kind === 'artist' ? 'artiste' : 'album'} size="small" /></span>
      <span>
        <span class="lien-site">Sillon · {GENRES[s.type]}</span>
        <strong>{s.title}</strong>
        {#if s.type !== 'artist'}<span class="lien-desc">{s.artist}</span>{/if}
        {#if s.listen}<a class="plbls-button plbls-button--compact sillon-carte-ecouter" href={ecouter(s)}><Icon name="lecture" size={14} /> Écouter</a>{/if}
      </span>
    </div>
  {:else if p}
    <a class="lien-apercu" class:avec-image={!!p.image} href={p.url} target="_blank" rel="noopener noreferrer nofollow">
      <span>
        {#if p.site_name}<span class="lien-site">{p.site_name}</span>{/if}
        <strong>{p.title}</strong>
        {#if p.description}<span class="lien-desc">{p.description}</span>{/if}
      </span>
      {#if p.image}<img src={p.image} alt="" loading="lazy" referrerpolicy="no-referrer" />{/if}
    </a>
  {/if}
{/await}
{/if}
