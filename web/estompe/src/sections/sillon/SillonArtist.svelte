<script lang="ts">
  import { ApiError } from '../../lib/api'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonAlbums from './SillonAlbums.svelte'
  import SillonCover from './SillonCover.svelte'
  import { echec } from './sillon.svelte'
  import { sillonApi, type FicheArtiste } from './sillon'

  let { id }: { id: string } = $props()

  let fiche = $state<FicheArtiste | null>(null)
  let manque = $state(false)
  const presents = $derived(fiche?.albums.filter((a) => a.available) ?? [])
  const absents = $derived(fiche?.albums.filter((a) => !a.available) ?? [])

  $effect(() => {
    let gone = false
    fiche = null
    manque = false
    sillonApi.artiste(id).then((f) => { if (!gone) fiche = f }).catch((e) => { if (gone) return; manque = true; if (!(e instanceof ApiError && e.status === 404)) echec(e, 'Cet artiste est indisponible') })
    return () => { gone = true }
  })
</script>

{#if fiche}
  <div class="film-page film-page--flush">
    <div class="film-hero sillon-hero">
      <a class="film-back plbls-button plbls-button--compact plbls-button--contour" href={hrefs.musique('artists')}><Icon name="gauche" size={14} /> Artistes</a>
      <div class="film-hero-body">
        <span class="film-card-art film-body-poster sillon-portrait"><SillonCover id={fiche.id} title={fiche.name} image={fiche.image} kind="artiste" size="large" /></span>
        <div class="film-info">
          <p class="v-eyebrow">{[`${presents.length} album${presents.length > 1 ? 's' : ''}`, ...fiche.genres.slice(0, 3)].join(' · ')}</p>
          <h1>{fiche.name}</h1>
          {#if fiche.overview}<p class="film-overview sillon-resume">{fiche.overview}</p>{/if}
        </div>
      </div>
    </div>
    <section class="film-body">
      <h2 class="v-titre">Albums</h2>
      {#if presents.length}<SillonAlbums albums={presents} artiste={false} />{:else}<p class="v-note">Aucun album présent.</p>{/if}
      {#if absents.length}
        <h2 class="v-titre">Absents de la bibliothèque</h2>
        <SillonAlbums albums={absents} artiste={false} />
      {/if}
    </section>
  </div>
{:else if manque}
  <div class="film-page"><div class="film-missing"><Icon name="alerte" /><span class="grow">Cet artiste est indisponible.</span><a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.musique('artists')}>Retour aux artistes</a></div></div>
{:else}
  <div class="film-page"><Loader /></div>
{/if}

<style>
  .sillon-resume { display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
</style>
