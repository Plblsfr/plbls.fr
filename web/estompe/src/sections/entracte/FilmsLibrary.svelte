<script lang="ts">
  import { filmsApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import { films } from './films.svelte'
  import { episodeLabel, percent, remaining, shortDuration, yearOptions, type FilmItem } from './films'

  let { section }: { section?: string } = $props()

  const PAGE = 40
  const ACCUEIL = 14
  const YEARS = yearOptions(new Date().getFullYear())
  let genre = $state('')
  let year = $state('')
  let sort = $state('recent')
  let genres = $state<string[]>([])
  let items = $state<FilmItem[]>([])
  let total = $state(0)
  let loading = $state(true)
  let turn = 0

  const fromList = $derived(section === 'continue' || section === 'seen')
  const accueil = $derived(!section)
  const heading = $derived(({ movies: 'Films', series: 'Séries', continue: 'Reprendre', seen: 'Historique' } as Record<string, string>)[section ?? ''] ?? '')
  const filtering = $derived(!!(genre || year))
  const gridHeading = $derived(accueil ? (filtering ? genre || 'Résultats' : 'Ajouts récents') : '')
  const showResume = $derived(accueil && !filtering && films.resume.length > 0)
  const une = $derived(films.resume[0])
  const suite = $derived(films.resume.slice(1, 4))
  const fail = (e: unknown) => notify.error(e, 'La bibliothèque est indisponible')
  const meta = (i: FilmItem) => i.kind === 'series' ? 'Série' : [i.year, i.duration ? shortDuration(i.duration) : ''].filter(Boolean).join(' · ')
  const target = (i: FilmItem) => hrefs.films('item', i.id)
  const fromSeries = (i: FilmItem) => i.kind === 'episode' && !!i.series_id
  const fiche = (i: FilmItem) => hrefs.films('item', fromSeries(i) ? i.series_id! : i.id)
  const sur = (i: FilmItem) => (i.kind === 'episode' ? ['Série', episodeLabel(i)].filter(Boolean).join(' · ') : 'Film')
  const reste = (i: FilmItem) => [i.series_title ? i.title : '', i.progress ? remaining(i.progress) : ''].filter(Boolean).join(' · ')

  function params(start: number): Record<string, string> {
    const p: Record<string, string> = { start: String(start), limit: String(accueil && !filtering ? ACCUEIL : PAGE), sort }
    if (section === 'movies') p.kind = 'movie'
    if (section === 'series') p.kind = 'series'
    if (genre) p.genre = genre
    if (year) p.year = year
    return p
  }

  async function load(start: number) {
    const mine = ++turn
    loading = true
    try {
      if (fromList) {
        const r = section === 'seen' ? await filmsApi.seen() : await filmsApi.resume()
        if (mine !== turn) return
        items = r.items
        total = r.items.length
      } else {
        const r = await filmsApi.library(params(start))
        if (mine !== turn) return
        items = start ? [...items, ...r.items] : r.items
        total = r.total
      }
    } catch (e) { if (mine === turn) fail(e) } finally { if (mine === turn) loading = false }
  }

  $effect(() => { section; genre; year; sort; void load(0) })
  $effect(() => { void filmsApi.genres().then((g) => (genres = g.genres)).catch(() => {}) })
</script>

<div class="film-page">
  {#if !accueil}
    <div class="film-filters">
      <h2 class="v-titre film-filters-title">{heading}</h2>
      {#if !fromList}
        <label class="film-filter"><span class="v-sr">Genre</span>
          <select class="plbls-select film-select" bind:value={genre}><option value="">Tous les genres</option>{#each genres as g (g)}<option value={g}>{g}</option>{/each}</select>
        </label>
        <label class="film-filter"><span class="v-sr">Année</span>
          <select class="plbls-select film-select" bind:value={year}>{#each YEARS as y (y.value)}<option value={y.value}>{y.value ? y.label : 'Toutes les années'}</option>{/each}</select>
        </label>
        <label class="film-filter"><span class="v-sr">Tri</span>
          <select class="plbls-select film-select" bind:value={sort}><option value="recent">Ajout récent</option><option value="title">Titre</option><option value="year">Année</option></select>
        </label>
      {/if}
    </div>
  {/if}

  {#if showResume && une}
    <section class="film-section">
      <div class="film-section-head"><h2 class="v-titre"><span class="film-surligne">Reprendre</span></h2>{#if films.resume.length > 1}<a class="v-note film-all" href={hrefs.films('continue')}>Tout voir · {films.resume.length}</a>{/if}</div>
      <div class="film-une">
        <article class="film-une-grand">
          <span class="film-une-art"><FilmPoster id={une.id} title={une.series_title ?? une.title} image={une.backdrop} type="fond" width={1280} /></span>
          <p class="film-une-sur">{sur(une)}</p>
          <div class="film-une-bas">
            <h3>{une.series_title ?? une.title}</h3>
            {#if reste(une)}<p class="film-une-reste">{reste(une)}</p>{/if}
            {#if une.progress}<span class="film-bar"><span style="width:{percent(une.progress)}"></span></span>{/if}
            <div class="film-une-actions">
              <a class="plbls-button film-btn-rose" href={hrefs.films('watch', une.id)}><Icon name="lecture" size={18} /> Reprendre</a>
              <a class="plbls-button film-btn-clair" href={fiche(une)}>Voir la fiche</a>
            </div>
          </div>
        </article>
        {#if suite.length}
          <div class="film-une-suite">
            {#each suite as r (r.id)}
              <a class="film-reprise" href={hrefs.films('watch', r.id)}>
                <span class="film-reprise-art"><FilmPoster id={r.id} title={r.series_title ?? r.title} image={r.backdrop} type="fond" width={480} /><span class="film-reprise-play" aria-hidden="true"><Icon name="lecture" size={14} /></span></span>
                <span class="film-reprise-meta">
                  <strong class="truncate">{r.series_title ?? r.title}{episodeLabel(r) ? ` · ${episodeLabel(r)}` : ''}</strong>
                  {#if r.progress}<span class="v-mono v-mute">{remaining(r.progress)}</span>{/if}
                  <span class="film-bar"><span style="width:{percent(r.progress)}"></span></span>
                </span>
              </a>
            {/each}
          </div>
        {/if}
      </div>
    </section>
  {/if}

  <section class="film-section">
    {#if gridHeading}
      <div class="film-section-head"><h2 class="v-titre">{gridHeading}</h2>
        {#if filtering}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => { genre = ''; year = '' }}>Effacer le filtre</button>
        {:else}<a class="v-note film-all" href={hrefs.films('movies')}>Tous les films</a><a class="v-note film-all" href={hrefs.films('series')}>Toutes les séries</a>{/if}
      </div>
    {/if}
    {#if loading && !items.length}
      <Loader />
    {:else if !items.length}
      <div class="v-empty"><Icon name="film" size={28} /><p class="v-note">{fromList ? 'Rien pour l’instant.' : 'Aucun titre ne correspond.'}</p></div>
    {:else}
      <div class="film-grid">
        {#each items as i (i.id)}
          <a class="film-card" href={target(i)}>
            <span class="film-card-art">
              <FilmPoster id={fromSeries(i) ? i.series_id! : i.id} title={i.series_title ?? i.title} image={fromSeries(i) || i.poster} />
              {#if i.available === false}<span class="film-card-vu film-card-absent">Indisponible</span>
              {:else if i.progress?.seen}<span class="film-card-vu"><Icon name="ok" size={14} /> Vu</span>{/if}
            </span>
            <strong class="truncate">{i.series_title ?? i.title}</strong>
            <span class="v-mono v-mute truncate">{episodeLabel(i) || meta(i)}</span>
            {#if section === 'seen' && i.seen_at}<span class="v-note truncate">vu le {new Date(i.seen_at).toLocaleDateString('fr-FR')}</span>{/if}
            {#if i.progress && !i.progress.seen}<span class="v-mono film-left">{remaining(i.progress)}</span><span class="film-bar"><span style="width:{percent(i.progress)}"></span></span>{/if}
          </a>
        {/each}
      </div>
      {#if !fromList && (!accueil || filtering) && items.length < total}
        <button class="plbls-button plbls-button--contour film-more" disabled={loading} onclick={() => void load(items.length)}>Afficher plus</button>
      {/if}
    {/if}
  </section>

  {#if accueil && !filtering && genres.length}
    <section class="film-section">
      <h2 class="v-titre">Parcourir par genre</h2>
      <div class="film-genres">
        {#each genres as g (g)}<button class="film-genre" onclick={() => (genre = g)}>{g}</button>{/each}
      </div>
    </section>
  {/if}
</div>

<style>
  .film-card-absent { background: var(--papier); color: var(--encre-douce, inherit); }
</style>
