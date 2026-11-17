<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { clock } from '../../blocks/musique/music'
  import SillonAlbums from './SillonAlbums.svelte'
  import SillonCover from './SillonCover.svelte'
  import { notify } from '../../lib/ui'
  import { echec, lecteur } from './sillon.svelte'
  import { sillonApi, type Album } from './sillon'

  let albums = $state<Album[]>([])
  let suite = $state<string | null>(null)
  let loading = $state(true)
  let tirage = $state(false)

  const rp = $derived(lecteur.aReprendre ? lecteur.piste : null)

  async function charger(cursor: string | null) {
    loading = true
    try {
      const r = await sillonApi.recents(cursor, 40)
      albums = cursor ? [...albums, ...r.items] : r.items
      suite = r.next_cursor
    } catch (e) { echec(e, 'La bibliothèque est indisponible') } finally { loading = false }
  }

  async function toutMelanger() {
    tirage = true
    try {
      if (!(await lecteur.melangerTout())) notify.fail('La bibliothèque est vide pour l’instant.')
    } catch (e) { echec(e, 'La lecture aléatoire n’a pas pu démarrer') } finally { tirage = false }
  }

  $effect(() => { void charger(null) })
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Accueil</h2>
    <button class="plbls-button" disabled={tirage} onclick={() => void toutMelanger()} title="Lance toute la bibliothèque en aléatoire, selon tes goûts, sans fin"><Icon name="shuffle" size={18} /> Tout mélanger</button>
  </div>

  {#if rp}
    <section class="film-section">
      <h2 class="v-titre">Reprendre</h2>
      <div class="film-resume">
        <button class="film-resume-card sillon-reprise" onclick={() => lecteur.reprendre()} aria-label={`Reprendre ${rp.title}, ${rp.artist.name}`}>
          <span class="film-resume-art"><SillonCover id={rp.album.id} title={rp.album.title} image={rp.album.cover} size="large" /></span>
          <span class="film-resume-play" aria-hidden="true"><Icon name="lecture" size={20} /></span>
          <span class="film-resume-meta"><strong class="truncate">{rp.title}</strong><span class="v-mono">{rp.artist.name} · {rp.album.title}{lecteur.position ? ` · ${clock(lecteur.position)}` : ''}</span></span>
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
