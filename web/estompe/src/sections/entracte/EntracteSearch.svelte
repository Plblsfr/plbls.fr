<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import EntractePoster from './EntractePoster.svelte'
  import RequestModal from './RequestModal.svelte'
  import { entracteApi } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { splitSearch, stateLabel, stateTone, TMDB_ATTRIBUTION, type SearchResult } from './entracte'

  let q = $state('')
  let items = $state<SearchResult[]>([])
  let tmdbError = $state(false)
  let loading = $state(false)
  let asked = $state<SearchResult | null>(null)
  let turn = 0
  let version = $state(0)

  const canRequest = $derived(!!app.entracteMe?.permissions.request)
  const groups = $derived(splitSearch(items))

  $effect(() => {
    const text = q.trim()
    void version
    if (!text) { items = []; return }
    const mine = ++turn
    const t = setTimeout(async () => {
      loading = true
      try {
        const r = await entracteApi.search(text)
        if (mine !== turn) return
        items = r.items
        tmdbError = r.tmdb_error
      } catch (e) { if (mine === turn) notify.error(e, 'La recherche est indisponible') } finally { if (mine === turn) loading = false }
    }, 300)
    return () => clearTimeout(t)
  })

  const meta = (i: SearchResult) => [i.kind === 'series' ? 'Série' : 'Film', i.year].filter(Boolean).join(' · ')
  const done = () => { version++ }
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Rechercher et demander</h2>
    <span class="film-search"><Icon name="cherche" size={16} /><!-- svelte-ignore a11y_autofocus --><input class="plbls-input" type="search" aria-label="Rechercher un film ou une série" placeholder="Un film, une série, même absent du serveur…" maxlength="100" autofocus bind:value={q} /></span>
  </div>
  {#if tmdbError}<p class="v-note" role="status">La recherche des titres absents est indisponible pour l’instant : seuls les titres du serveur sont affichés.</p>{/if}
  {#if loading && !items.length}
    <Loader />
  {:else if q.trim() && !items.length}
    <div class="v-empty"><Icon name="film" size={28} /><p class="v-note">Aucun titre ne correspond.</p></div>
  {/if}
  {#if groups.present.length}
    <section class="film-section">
      <h2 class="v-titre">Sur le serveur</h2>
      <div class="film-grid">
        {#each groups.present as i (`${i.kind}-${i.tmdb_id}`)}
          <div class="film-card">
            <a class="film-card-art" href={hrefs.films('item', i.id ?? '')}><FilmPoster id={i.id ?? ''} title={i.title} image={!!i.poster} /></a>
            <strong class="truncate">{i.title}</strong>
            <span class="v-mono v-mute truncate">{meta(i)}</span>
            {#if canRequest && i.kind === 'series'}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (asked = i)}>Demander des saisons</button>{/if}
          </div>
        {/each}
      </div>
    </section>
  {/if}
  {#if groups.absent.length}
    <section class="film-section">
      <h2 class="v-titre">Absents du serveur</h2>
      <div class="film-grid">
        {#each groups.absent as i (`${i.kind}-${i.tmdb_id}`)}
          <div class="film-card film-card--absent">
            <span class="film-card-art"><EntractePoster url={i.poster_url} title={i.title} seed={`${i.kind}-${i.tmdb_id}`} /></span>
            <strong class="truncate">{i.title}</strong>
            <span class="v-mono v-mute truncate">{meta(i)}</span>
            {#if i.request && (i.request.state === 'en_attente' || i.request.state === 'acceptee')}
              <span class="v-tag {stateTone(i.request.state)}">Demandé · {stateLabel(i.request.state)}</span>
              {#if canRequest}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (asked = i)}>Me prévenir aussi</button>{/if}
            {:else if canRequest && i.requestable}
              <button class="plbls-button plbls-button--compact" onclick={() => (asked = i)}><Icon name="envoi" size={14} /> Demander</button>
            {/if}
          </div>
        {/each}
      </div>
    </section>
  {/if}
  <p class="v-note film-tmdb-note">{TMDB_ATTRIBUTION}</p>
</div>
{#if asked}
  <RequestModal kind={asked.kind === 'series' ? 'series' : 'movie'} tmdbId={asked.tmdb_id} title={asked.title} onclose={() => (asked = null)} ondone={done} />
{/if}

<style>
  .film-card--absent .film-card-art { opacity: 0.85; }
  .film-tmdb-note { margin-top: var(--espace-5); font-size: 0.75rem; }
</style>
