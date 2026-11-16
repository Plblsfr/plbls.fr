<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonAlbums from './SillonAlbums.svelte'
  import SillonCover from './SillonCover.svelte'
  import SillonTracks from './SillonTracks.svelte'
  import { echec } from './sillon.svelte'
  import { sillonApi, type Resultats } from './sillon'

  let q = $state('')
  let res = $state<Resultats | null>(null)
  let loading = $state(false)
  let tour = 0
  const vide = $derived(!!res && !res.artists.length && !res.albums.length && !res.tracks.length)

  $effect(() => {
    const t = q.trim()
    const mine = ++tour
    if (!t) { res = null; loading = false; return }
    loading = true
    sillonApi.recherche(t).then((r) => { if (mine === tour) res = r }).catch((e) => { if (mine === tour) echec(e, 'La recherche est indisponible') }).finally(() => { if (mine === tour) loading = false })
  })
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Rechercher</h2>
    <span class="film-search sillon-recherche"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" aria-label="Rechercher dans la musique" placeholder="Artiste, album, titre…" maxlength="100" bind:value={q} /></span>
  </div>
  {#if loading && !res}
    <Loader />
  {:else if vide}
    <div class="v-empty"><Icon name="cherche" size={28} /><p class="v-note">Aucun résultat pour « {q.trim()} ».</p></div>
  {:else if res}
    {#if res.tracks.length}
      <section class="film-section"><h2 class="v-titre">Titres</h2><SillonTracks pistes={res.tracks} /></section>
    {/if}
    {#if res.artists.length}
      <section class="film-section">
        <h2 class="v-titre">Artistes</h2>
        <div class="film-grid sillon-grid">
          {#each res.artists as a (a.id)}
            <a class="film-card sillon-artiste" href={hrefs.musique('artist', a.id)}>
              <span class="film-card-art sillon-rond"><SillonCover id={a.id} title={a.name} image={a.image} kind="artiste" /></span>
              <strong class="truncate">{a.name}</strong>
            </a>
          {/each}
        </div>
      </section>
    {/if}
    {#if res.albums.length}
      <section class="film-section"><h2 class="v-titre">Albums</h2><SillonAlbums albums={res.albums} /></section>
    {/if}
  {:else}
    <p class="v-note">Cherche un artiste, un album ou un titre de la bibliothèque, avec ou sans accents.</p>
  {/if}
</div>

<style>
  .sillon-recherche { flex: 1 1 360px; }
</style>
