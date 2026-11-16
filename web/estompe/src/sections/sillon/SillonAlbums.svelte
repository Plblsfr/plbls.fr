<script lang="ts">
  import { ctx } from '../../lib/ctxmenu.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { copierLien, menuContenu } from './sillon.svelte'
  import SillonCover from './SillonCover.svelte'
  import { anneeEtType, type Album } from './sillon'

  let { albums, artiste = true }: { albums: Album[]; artiste?: boolean } = $props()
</script>

<div class="film-grid sillon-grid">
  {#each albums as a (a.id)}
    <a class="film-card" href={hrefs.musique('album', a.id)} use:ctx={() => [...(a.available ? menuContenu({ album: a.id }, a.title) : []), { label: 'Copier le lien', run: () => copierLien('album', a.id), sep: a.available }]}>
      <span class="film-card-art sillon-carre">
        <SillonCover id={a.id} title={a.title} image={a.cover} />
        {#if !a.available}<span class="film-card-vu sillon-absent">Indisponible</span>{/if}
      </span>
      <strong class="truncate">{a.title}</strong>
      <span class="v-mono v-mute truncate">{artiste ? a.artist.name : anneeEtType(a)}</span>
    </a>
  {/each}
</div>

