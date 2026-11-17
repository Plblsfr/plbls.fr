<script lang="ts">
  import SillonCover from './SillonCover.svelte'
  import type { Taille } from './sillon'

  let { albums, id, titre, size = 'medium' }: { albums: string[]; id: string; titre: string; size?: Taille } = $props()
</script>

{#if albums.length >= 4}
  <span class="sillon-mosaique">{#each albums.slice(0, 4) as a (a)}<span><SillonCover id={a} title="" image size="small" /></span>{/each}</span>
{:else if albums.length}
  <SillonCover id={albums[0]} title={titre} image {size} />
{:else}
  <SillonCover {id} title={titre} image={false} {size} />
{/if}

<style>
  .sillon-mosaique { position: absolute; inset: 0; display: grid; grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; }
  .sillon-mosaique > span { position: relative; overflow: hidden; }
</style>
