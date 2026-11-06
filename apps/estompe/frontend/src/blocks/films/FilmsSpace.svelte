<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import FilmDetail from './FilmDetail.svelte'
  import FilmPlayer from './FilmPlayer.svelte'
  import FilmsLibrary from './FilmsLibrary.svelte'

  let { section, id, onmenu }: { section?: string; id?: string; onmenu: () => void } = $props()
</script>

{#if section === 'watch' && id}
  {#key id}<FilmPlayer {id} />{/key}
{:else}
  <header class="main-head film-head">
    <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
    <h1>Films et séries</h1>
  </header>
  {#if section === 'item' && id}
    <FilmDetail {id} />
  {:else}
    <FilmsLibrary {section} />
  {/if}
{/if}
