<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import MessageList from '../../components/MessageList.svelte'
  import Composer from '../../components/Composer.svelte'
  import VideoCard from './VideoCard.svelte'
  import VideoLists from './VideoLists.svelte'
  import VideoSessions from './VideoSessions.svelte'
  import WatchPicker from '../regarder-ensemble/WatchPicker.svelte'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { openSearch } from '../../lib/search.svelte'
  import { channelKey, type Channel, type SessionKind } from '../../lib/types'
  import { videos } from './videos.svelte'
  import { artistOf, filterPosts, NO_FILTER, playLink, sessionPhase, sourceOf, when, type VideoFilter, type VideoSort } from './videos'
  import { PROVIDER_LABEL } from '../regarder-ensemble/providers'

  let { channel, serverId, onmenu }: { channel: Channel; serverId: string; onmenu: () => void } = $props()

  type Tab = 'videos' | 'lists' | 'sessions' | 'chat'
  const isMusic = $derived(channel.kind === 'music')
  const TABS = $derived<[Tab, string, 'video' | 'headphones' | 'list' | 'calendar' | 'reply'][]>([['videos', isMusic ? 'Morceaux' : 'Vidéos', isMusic ? 'headphones' : 'video'], ['lists', isMusic ? 'Playlists' : 'Listes', 'list'], ['sessions', isMusic ? 'Écoutes' : 'Séances', 'calendar'], ['chat', 'Discussion', 'reply']])
  let tab = $state<Tab>('videos')
  let layout = $state<'grid' | 'list'>('grid')
  let filter = $state<VideoFilter>({ ...NO_FILTER })
  let sort = $state<VideoSort>('recent')
  let link = $state('')
  let sending = $state(false)
  let picker = $state<{ links: string[]; kind: SessionKind } | null>(null)
  let planList = $state('')

  const board = $derived(videos.boards[channel.id])
  const posts = $derived(board?.posts ?? [])
  const shown = $derived(filterPosts(posts, filter, sort, board?.lists))
  const sources = $derived([...new Set(posts.map((p) => sourceOf(p.provider)))])
  const artists = $derived([...new Set(posts.map(artistOf).filter(Boolean))].sort((a, b) => a.localeCompare(b, 'fr')))
  const authors = $derived([...new Map(posts.map((p) => [p.author.id, p.author.pseudo])).entries()])
  const upcoming = $derived((board?.sessions ?? []).filter((s) => sessionPhase(s) === 'upcoming' || (s.started_at && Date.now() - Date.parse(s.starts_at) < 3 * 3600_000)).slice(0, 2))
  const key = $derived(channelKey(channel.id))
  const canSend = $derived(app.can(serverId, 'send_messages', channel.id))

  $effect(() => { void videos.load(channel.id) })

  async function publish(e: Event) {
    e.preventDefault()
    const v = link.trim()
    if (!v || sending) return
    sending = true
    try { await app.send(key, v); link = '' } catch (err) { notify.error(err, 'Publication impossible') } finally { sending = false }
  }
  const label = (s: string) => (s === 'twitch' ? 'Twitch' : (PROVIDER_LABEL[s] ?? s))
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1 class="grow truncate row"><Icon name={isMusic ? 'headphones' : 'video'} size={20} /> {channel.name}</h1>
  <button class="ib" onclick={openSearch} aria-label="Rechercher" title="Rechercher (Ctrl+K)"><Icon name="search" /></button>
</header>

{#if upcoming.length}
  <div class="vnext" role="status">
    {#each upcoming as s (s.id)}
      <button class="v-chip on vnext-item" onclick={() => (tab = 'sessions')}><Icon name="horloge" size={14} /> <strong class="truncate">{s.title}</strong> <span class="v-mono">{s.started_at ? 'en cours' : when(s.starts_at)}</span></button>
    {/each}
  </div>
{/if}

<div class="v-tabs vtabs" role="tablist" aria-label="Sections du salon">
  {#each TABS as [id, text, icon] (id)}
    <button role="tab" class="vtab" aria-selected={tab === id} onclick={() => (tab = id)}><Icon name={icon} size={16} /> {text}</button>
  {/each}
</div>

{#if tab === 'chat'}
  <MessageList convKey={key} />
  {#if canSend}<Composer convKey={key} placeholder={`Écrire dans #${channel.name}`} />{:else}<p class="v-note vno-send">Vous n'avez pas la permission d'écrire dans ce salon.</p>{/if}
{:else}
  <div class="vbody-wrap">
    {#if !board}
      <Loader />
    {:else if tab === 'videos'}
      {#if canSend}
        <form class="vpublish" onsubmit={publish}>
          <span class="vpublish-ic" aria-hidden="true"><Icon name="lien" size={16} /></span>
          <input class="plbls-input grow" bind:value={link} placeholder={isMusic ? 'Coller un lien YouTube Music (morceau, album, playlist)' : 'Coller un lien de vidéo (YouTube, Twitch, Vimeo, Dailymotion, fichier…)'} aria-label={isMusic ? 'Lien du morceau à publier' : 'Lien de la vidéo à publier'} maxlength="2000" />
          <button class="plbls-button" disabled={!link.trim() || sending}>Publier</button>
        </form>
      {/if}
      <div class="vfilters">
        <span class="vfilters-ic" aria-hidden="true"><Icon name="filtre" size={16} /></span>
        {#if isMusic}
          <select class="plbls-select" bind:value={filter.artist} aria-label="Filtrer par artiste"><option value="">Tous les artistes</option>{#each artists as a (a)}<option value={a}>{a}</option>{/each}</select>
          <select class="plbls-select" bind:value={filter.watched} aria-label="Filtrer selon l'écoute"><option value="all">Écoutés ou non</option><option value="unseen">Pas encore écoutés</option><option value="seen">Déjà écoutés (historique)</option></select>
        {:else}
          <select class="plbls-select" bind:value={filter.source} aria-label="Filtrer par source"><option value="">Toutes les sources</option>{#each sources as s (s)}<option value={s}>{label(s)}</option>{/each}</select>
          <select class="plbls-select" bind:value={filter.watched} aria-label="Filtrer selon la vue"><option value="all">Vues ou non</option><option value="unseen">Pas encore vues</option><option value="seen">Déjà vues</option></select>
        {/if}
        <select class="plbls-select" bind:value={filter.author} aria-label="Filtrer par auteur"><option value="">Tous les auteurs</option>{#each authors as [id, pseudo] (id)}<option value={id}>{pseudo}</option>{/each}</select>
        {#if board.lists.length}<select class="plbls-select" bind:value={filter.list} aria-label="Filtrer par liste"><option value="">Toutes les listes</option>{#each board.lists as l (l.id)}<option value={l.id}>{l.name}</option>{/each}</select>{/if}
        <select class="plbls-select" bind:value={sort} aria-label="Trier"><option value="recent">Plus récentes</option><option value="reactions">Plus de réactions</option>{#if isMusic}<option value="title">Titre (A→Z)</option>{/if}</select>
        <span class="grow"></span>
        <span class="v-seg vlayout">
          <button aria-pressed={layout === 'grid'} onclick={() => (layout = 'grid')} aria-label="Vue en grille" title="Grille"><Icon name="grid" size={16} /></button>
          <button aria-pressed={layout === 'list'} onclick={() => (layout = 'list')} aria-label="Vue en liste" title="Liste"><Icon name="list" size={16} /></button>
        </span>
      </div>
      <div class="vgrid" class:vlist-layout={layout === 'list'}>
        {#each shown as p (p.message_id)}
          <VideoCard post={p} channelId={channel.id} {serverId} lists={board.lists} compact={layout === 'list'} onwatch={(x) => (picker = { links: [playLink(x)], kind: x.provider === 'ytmusic' ? 'music' : 'watch' })} />
        {:else}
          <p class="v-empty v-note vempty">{posts.length ? (isMusic ? 'Aucun morceau ne correspond aux filtres.' : 'Aucune vidéo ne correspond aux filtres.') : (isMusic ? 'Aucun morceau pour le moment. Collez un lien YouTube Music ci-dessus pour lancer le salon.' : 'Aucune vidéo pour le moment. Collez un lien ci-dessus pour lancer le salon.')}</p>
        {/each}
      </div>
    {:else if tab === 'lists'}
      <VideoLists channelId={channel.id} {serverId} lists={board.lists} {posts} onwatch={(links, kind = 'watch') => (picker = { links, kind })} onplan={(id) => { planList = id; tab = 'sessions' }} />
    {:else}
      <VideoSessions channelId={channel.id} {serverId} sessions={board.sessions} {posts} lists={board.lists} {planList} onplanned={() => (planList = '')} />
    {/if}
  </div>
{/if}

{#if picker}<WatchPicker {serverId} links={picker.links} kind={picker.kind} onclose={() => (picker = null)} />{/if}
