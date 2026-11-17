<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { app } from '../../lib/store.svelte'
  import SillonAdd from './SillonAdd.svelte'
  import SillonAdditions from './SillonAdditions.svelte'
  import SillonAdmin from './SillonAdmin.svelte'
  import SillonAlbum from './SillonAlbum.svelte'
  import SillonArtist from './SillonArtist.svelte'
  import SillonArtists from './SillonArtists.svelte'
  import SillonChoixPlaylist from './SillonChoixPlaylist.svelte'
  import SillonHistory from './SillonHistory.svelte'
  import SillonHome from './SillonHome.svelte'
  import SillonNoAccess from './SillonNoAccess.svelte'
  import SillonPlaylist from './SillonPlaylist.svelte'
  import SillonQueue from './SillonQueue.svelte'
  import SillonSearch from './SillonSearch.svelte'
  import SillonTrack from './SillonTrack.svelte'
  import { aimes, collection, vue } from './sillon.svelte'

  let { section, id, onmenu, acces = true }: { section?: string; id?: string; onmenu: () => void; acces?: boolean } = $props()
  const droits = $derived(app.sillonMe?.permissions)
  const ouvert = $derived(acces && !vue.refuse)

  $effect(() => {
    if (!ouvert) return
    void aimes.charger()
    if (!collection.charge) void collection.charger()
  })
</script>

<header class="main-head film-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1>Sillon</h1>
</header>
{#if !ouvert}
  <SillonNoAccess />
{:else if section === 'album' && id}
  {#key id}<SillonAlbum {id} />{/key}
{:else if section === 'track' && id}
  {#key id}<SillonTrack {id} />{/key}
{:else if section === 'artist' && id}
  {#key id}<SillonArtist {id} />{/key}
{:else if section === 'playlist' && id}
  {#key id}<SillonPlaylist {id} />{/key}
{:else if section === 'likes'}
  <SillonPlaylist />
{:else if section === 'queue'}
  <SillonQueue />
{:else if section === 'history'}
  <SillonHistory />
{:else if section === 'artists'}
  <SillonArtists />
{:else if section === 'search'}
  <SillonSearch />
{:else if section === 'add' && droits?.add}
  <SillonAdd />
{:else if section === 'additions' && droits?.add}
  <div class="film-page"><div class="film-filters"><h2 class="v-titre film-filters-title">Mes ajouts</h2></div><section class="film-section"><SillonAdditions /></section></div>
{:else if section === 'admin' && droits?.admin}
  <SillonAdmin />
{:else}
  <SillonHome />
{/if}
{#if ouvert}<SillonChoixPlaylist />{/if}
