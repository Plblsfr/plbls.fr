<script lang="ts">
  import { hrefs } from '../../lib/route.svelte'
  import SillonCover from './SillonCover.svelte'
  import { anneeEtType, type Album } from './sillon'

  let { albums, artiste = true }: { albums: Album[]; artiste?: boolean } = $props()
</script>

<div class="film-grid sillon-grid">
  {#each albums as a (a.id)}
    <a class="film-card" href={hrefs.musique('album', a.id)}>
      <span class="film-card-art sillon-carre">
        <SillonCover id={a.id} title={a.title} image={a.cover} />
        {#if !a.available}<span class="film-card-vu sillon-absent">Indisponible</span>{/if}
      </span>
      <strong class="truncate">{a.title}</strong>
      <span class="v-mono v-mute truncate">{artiste ? a.artist.name : anneeEtType(a)}</span>
    </a>
  {/each}
</div>

