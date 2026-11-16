<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import SillonAlbum from './SillonAlbum.svelte'
  import SillonArtist from './SillonArtist.svelte'
  import SillonArtists from './SillonArtists.svelte'
  import SillonHome from './SillonHome.svelte'
  import SillonNoAccess from './SillonNoAccess.svelte'
  import SillonSearch from './SillonSearch.svelte'
  import { vue } from './sillon.svelte'

  let { section, id, onmenu, acces = true }: { section?: string; id?: string; onmenu: () => void; acces?: boolean } = $props()
</script>

<header class="main-head film-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1>Sillon</h1>
</header>
{#if !acces || vue.refuse}
  <SillonNoAccess />
{:else if section === 'album' && id}
  {#key id}<SillonAlbum {id} />{/key}
{:else if section === 'artist' && id}
  {#key id}<SillonArtist {id} />{/key}
{:else if section === 'artists'}
  <SillonArtists />
{:else if section === 'search'}
  <SillonSearch />
{:else}
  <SillonHome />
{/if}
