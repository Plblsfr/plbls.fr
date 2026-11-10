<script lang="ts">
  import { entracteOn, filmsApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import { films } from './films.svelte'
  import { episodeLabel, percent, remaining, shortDuration, yearOptions, type FilmItem } from './films'

  let { section }: { section?: string } = $props()

  const PAGE = 40
  const YEARS = yearOptions(new Date().getFullYear())
  let q = $state('')
  let genre = $state('')
  let year = $state('')
  let sort = $state('recent')
  let genres = $state<string[]>([])
  let items = $state<FilmItem[]>([])
  let total = $state(0)
  let loading = $state(true)
  let turn = 0

  const fromList = $derived(section === 'continue' || section === 'seen')
  const heading = $derived(({ movies: 'Films', series: 'Séries', continue: 'Reprendre', seen: entracteOn() ? 'Historique' : 'Déjà vus' } as Record<string, string>)[section ?? ''] ?? 'Accueil')
  const searching = $derived(!!(q.trim() || genre || year))
  const gridHeading = $derived(searching ? 'Résultats' : section ? '' : 'Ajouts récents')
  const showResume = $derived(!section && !searching && films.resume.length > 0)
  const fail = (e: unknown) => notify.error(e, 'La bibliothèque est indisponible')
  const meta = (i: FilmItem) => i.kind === 'series' ? 'Série' : [i.year, i.duration ? shortDuration(i.duration) : ''].filter(Boolean).join(' · ')
  const target = (i: FilmItem) => hrefs.films('item', i.id)
  const fromSeries = (i: FilmItem) => i.kind === 'episode' && !!i.series_id

  function params(start: number): Record<string, string> {
    const p: Record<string, string> = { start: String(start), limit: String(PAGE), sort }
    if (section === 'movies') p.kind = 'movie'
    if (section === 'series') p.kind = 'series'
    if (q.trim()) p.q = q.trim()
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

  $effect(() => { section; q; genre; year; sort; void load(0) })
  $effect(() => { void filmsApi.genres().then((g) => (genres = g.genres)).catch(() => {}); void films.refresh().catch(() => {}) })
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">{heading}</h2>
    {#if !fromList}
      <span class="film-search"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" aria-label="Rechercher un titre" placeholder="Rechercher un film, une série…" maxlength="100" bind:value={q} /></span>
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

  {#if showResume}
    <section class="film-section">
      <div class="film-section-head"><h2 class="v-titre">Reprendre</h2><a class="v-note film-all" href={hrefs.films('continue')}>Tout voir</a></div>
      <div class="film-resume">
        {#each films.resume as r (r.id)}
          <a class="film-resume-card" href={hrefs.films('watch', r.id)}>
            <span class="film-resume-art"><FilmPoster id={r.id} title={r.series_title ?? r.title} image={r.backdrop} type="fond" width={480} /></span>
            <span class="film-resume-play" aria-hidden="true"><Icon name="lecture" size={20} /></span>
            <span class="film-resume-meta"><strong class="truncate">{r.series_title ?? r.title}{episodeLabel(r) ? ` · ${episodeLabel(r)}` : ''}</strong>{#if r.progress}<span class="v-mono">{remaining(r.progress)}</span>{/if}<span class="film-bar"><span style="width:{percent(r.progress)}"></span></span></span>
          </a>
        {/each}
      </div>
    </section>
  {/if}

  <section class="film-section">
    {#if gridHeading}<h2 class="v-titre">{gridHeading}</h2>{/if}
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
      {#if !fromList && items.length < total}
        <button class="plbls-button plbls-button--contour film-more" disabled={loading} onclick={() => void load(items.length)}>Afficher plus</button>
      {/if}
    {/if}
  </section>
</div>

<style>
  .film-card-absent { background: var(--papier); color: var(--encre-douce, inherit); }
</style>
