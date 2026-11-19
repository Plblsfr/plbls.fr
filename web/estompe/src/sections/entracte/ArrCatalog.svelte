<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi } from '../../lib/api'
  import { notify } from '../../lib/ui'
  import ArrAdd from './ArrAdd.svelte'
  import { bytes, filterTitles, serviceName, serviceOf, titleLabel, titleState, type ArrError, type ArrQueueItem, type ArrService, type ArrTitle, type CatalogFilter } from './arr'
  import type { RequestKind, SearchResult } from './entracte'

  let { titles, errors, queue, onsheet, onreload }: { titles: ArrTitle[] | null; errors: ArrError[]; queue: ArrQueueItem[]; onsheet: (service: ArrService, id: number) => void; onreload: () => void } = $props()

  const PAS = 60
  let q = $state('')
  let results = $state<SearchResult[] | null>(null)
  let searching = $state(false)
  let adding = $state<{ kind: RequestKind; tmdbId: number; title: string } | null>(null)
  let filtre = $state<CatalogFilter>({ service: '', state: '', text: '' })
  let limite = $state(PAS)

  const visibles = $derived(filterTitles(titles ?? [], filtre))
  const compte = (s: ArrService) => (titles ?? []).filter((t) => t.service === s).length
  const connu = (r: SearchResult) => (titles ?? []).find((t) => t.service === serviceOf(r.kind === 'movie' ? 'movie' : 'series') && t.tmdb_id === r.tmdb_id)
  $effect(() => { void filtre.service; void filtre.state; void filtre.text; limite = PAS })

  async function find() {
    const texte = q.trim()
    if (texte.length < 2) return
    searching = true
    try {
      results = (await entracteApi.search(texte)).items.filter((r) => r.kind !== 'episode')
    } catch (e) { results = []; notify.error(e, 'La recherche est indisponible') } finally { searching = false }
  }
</script>

<section class="film-section">
  <h2 class="v-titre">Ajouter un titre</h2>
  <form class="film-filters" onsubmit={(e) => { e.preventDefault(); void find() }}>
    <label class="film-search cat-cherche"><Icon name="cherche" size={16} /><span class="v-sr">Film ou série à ajouter</span><input class="plbls-input" type="search" placeholder="Un film ou une série à ajouter à Radarr ou Sonarr" maxlength="100" bind:value={q} /></label>
    <button type="submit" class="plbls-button plbls-button--compact" disabled={searching || q.trim().length < 2}>Chercher</button>
  </form>
  {#if searching}
    <Loader />
  {:else if results}
    {#if !results.length}
      <p class="v-note">Aucun film ni série de ce nom.</p>
    {:else}
      <ul class="adm-list">
        {#each results.slice(0, 12) as r (`${r.kind}:${r.tmdb_id}`)}
          {@const deja = connu(r)}
          <li class="adm-item">
            <span class="grow adm-body"><strong>{r.title}{r.year ? ` (${r.year})` : ''}</strong><span class="v-note">{r.kind === 'movie' ? 'Film' : 'Série'}{deja ? ` · déjà dans ${serviceName(deja.service)}` : r.present ? ' · dans la bibliothèque' : ''}</span></span>
            {#if deja}
              <button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Gérer ${r.title}`} onclick={() => onsheet(deja.service, deja.id)}>Gérer</button>
            {:else}
              <button class="plbls-button plbls-button--compact" aria-label={`Ajouter ${r.title}`} onclick={() => (adding = { kind: r.kind === 'movie' ? 'movie' : 'series', tmdbId: r.tmdb_id, title: r.title })}><Icon name="plus" size={14} /> Ajouter…</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<section class="film-section">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Catalogue géré</h2>
    <div class="v-seg adm-filtres" role="group" aria-label="Type">
      <button aria-pressed={filtre.service === ''} onclick={() => (filtre.service = '')}>Tout · {titles?.length ?? 0}</button>
      <button aria-pressed={filtre.service === 'radarr'} onclick={() => (filtre.service = 'radarr')}>Films · {compte('radarr')}</button>
      <button aria-pressed={filtre.service === 'sonarr'} onclick={() => (filtre.service = 'sonarr')}>Séries · {compte('sonarr')}</button>
    </div>
    <div class="v-seg adm-filtres" role="group" aria-label="État">
      <button aria-pressed={filtre.state === ''} onclick={() => (filtre.state = '')}>Tous</button>
      <button aria-pressed={filtre.state === 'missing'} onclick={() => (filtre.state = 'missing')}>Manquants</button>
      <button aria-pressed={filtre.state === 'unmonitored'} onclick={() => (filtre.state = 'unmonitored')}>Non surveillés</button>
    </div>
    <label class="film-search"><Icon name="cherche" size={16} /><span class="v-sr">Filtrer par titre</span><input class="plbls-input" type="search" placeholder="Titre" maxlength="100" bind:value={filtre.text} /></label>
  </div>
  {#each errors as e (e.service)}<p class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {serviceName(e.service)} ne répond pas : {e.message}</p>{/each}
  {#if !titles}
    <Loader />
  {:else if !visibles.length}
    <div class="v-empty"><Icon name="film" size={28} /><p class="v-note">Aucun titre.</p></div>
  {:else}
    <ul class="adm-list">
      {#each visibles.slice(0, limite) as t (`${t.service}:${t.id}`)}
        {@const etat = titleState(t, queue)}
        <li class="adm-item">
          <span class="grow adm-body"><strong>{titleLabel(t)}</strong>
            <span class="v-note">{t.service === 'radarr' ? 'Film' : `Série · ${t.episode_files ?? 0} épisode${(t.episode_files ?? 0) > 1 ? 's' : ''} sur ${t.episodes ?? 0}`}{t.monitored ? '' : ' · non surveillé'}</span>
          </span>
          <span class="v-tag {etat.tone}">{etat.label}</span>
          <span class="adm-mono cat-taille">{bytes(t.size)}</span>
          <button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Gérer ${t.title}`} onclick={() => onsheet(t.service, t.id)}>Gérer</button>
        </li>
      {/each}
    </ul>
    {#if visibles.length > limite}
      <button class="plbls-button plbls-button--compact plbls-button--contour cat-plus" onclick={() => (limite += PAS)}>Afficher plus ({visibles.length - limite} restants)</button>
    {/if}
  {/if}
</section>
{#if adding}<ArrAdd kind={adding.kind} tmdbId={adding.tmdbId} title={adding.title} onclose={() => (adding = null)} ondone={() => { results = null; q = ''; onreload() }} />{/if}

<style>
  .cat-cherche { flex: 1 1 320px; max-width: 560px; }
  .cat-taille { flex: 0 0 72px; text-align: right; }
  .cat-plus { align-self: center; }
</style>
