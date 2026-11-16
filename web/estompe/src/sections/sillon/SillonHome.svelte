<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { clock } from '../../blocks/musique/music'
  import SillonAlbums from './SillonAlbums.svelte'
  import SillonCover from './SillonCover.svelte'
  import { echec, lecteur } from './sillon.svelte'
  import { sillonApi, type Album } from './sillon'

  let albums = $state<Album[]>([])
  let suite = $state<string | null>(null)
  let loading = $state(true)

  const reprise = $derived(lecteur.piste ? null : lecteur.reprise?.file[lecteur.reprise.index] ? lecteur.reprise : null)
  const rp = $derived(reprise ? reprise.file[reprise.index] : null)

  async function charger(cursor: string | null) {
    loading = true
    try {
      const r = await sillonApi.recents(cursor, 40)
      albums = cursor ? [...albums, ...r.items] : r.items
      suite = r.next_cursor
    } catch (e) { echec(e, 'La bibliothèque est indisponible') } finally { loading = false }
  }

  $effect(() => { void charger(null) })
</script>

<div class="film-page">
  <div class="film-filters"><h2 class="v-titre film-filters-title">Accueil</h2></div>

  {#if reprise && rp}
    <section class="film-section">
      <h2 class="v-titre">Reprendre</h2>
      <div class="film-resume">
        <button class="film-resume-card sillon-reprise" onclick={() => lecteur.reprendreSauvegarde()} aria-label={`Reprendre ${rp.title}, ${rp.artist.name}`}>
          <span class="film-resume-art"><SillonCover id={rp.album.id} title={rp.album.title} image={rp.album.cover} size="large" /></span>
          <span class="film-resume-play" aria-hidden="true"><Icon name="lecture" size={20} /></span>
          <span class="film-resume-meta"><strong class="truncate">{rp.title}</strong><span class="v-mono">{rp.artist.name} · {rp.album.title}{reprise.position ? ` · ${clock(reprise.position)}` : ''}</span></span>
        </button>
      </div>
    </section>
  {/if}

  <section class="film-section">
    <h2 class="v-titre">Ajouts récents</h2>
    {#if loading && !albums.length}
      <Loader />
    {:else if !albums.length}
      <div class="v-empty"><Icon name="note" size={28} /><p class="v-note">La bibliothèque est vide pour l’instant.</p></div>
    {:else}
      <SillonAlbums {albums} />
      {#if suite}<button class="plbls-button plbls-button--contour film-more" disabled={loading} onclick={() => void charger(suite)}>Afficher plus</button>{/if}
    {/if}
  </section>
</div>

<style>
  .sillon-reprise { width: 100%; padding: 0; border: 0; font: inherit; text-align: left; cursor: pointer; }
</style>
