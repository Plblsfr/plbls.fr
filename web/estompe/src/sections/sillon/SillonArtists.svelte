<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonCover from './SillonCover.svelte'
  import { echec } from './sillon.svelte'
  import { sillonApi, type Artiste } from './sillon'

  let artistes = $state<Artiste[]>([])
  let suite = $state<string | null>(null)
  let loading = $state(true)

  async function charger(cursor: string | null) {
    loading = true
    try {
      const r = await sillonApi.artistes(cursor)
      artistes = cursor ? [...artistes, ...r.items] : r.items
      suite = r.next_cursor
    } catch (e) { echec(e, 'La liste des artistes est indisponible') } finally { loading = false }
  }

  $effect(() => { void charger(null) })
</script>

<div class="film-page">
  <div class="film-filters"><h2 class="v-titre film-filters-title">Artistes</h2></div>
  <section class="film-section">
    {#if loading && !artistes.length}
      <Loader />
    {:else if !artistes.length}
      <div class="v-empty"><Icon name="perso" size={28} /><p class="v-note">Aucun artiste pour l’instant.</p></div>
    {:else}
      <div class="film-grid sillon-grid">
        {#each artistes as a (a.id)}
          <a class="film-card sillon-artiste" href={hrefs.musique('artist', a.id)}>
            <span class="film-card-art sillon-rond"><SillonCover id={a.id} title={a.name} image={a.image} kind="artiste" /></span>
            <strong class="truncate">{a.name}</strong>
            <span class="v-mono v-mute">{a.album_count} album{a.album_count > 1 ? 's' : ''}</span>
          </a>
        {/each}
      </div>
      {#if suite}<button class="plbls-button plbls-button--contour film-more" disabled={loading} onclick={() => void charger(suite)}>Afficher plus</button>{/if}
    {/if}
  </section>
</div>
