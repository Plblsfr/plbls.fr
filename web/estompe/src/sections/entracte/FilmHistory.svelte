<script lang="ts">
  import { filmsApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import { episodeLabel, episodesLabel, historyDays, shortDuration, type FilmItem, type HistoryEntry } from './films'

  let items = $state<FilmItem[]>([])
  let total = $state(0)
  let loading = $state(true)
  let turn = 0

  const days = $derived(historyDays(items, new Date()))
  const serie = (e: HistoryEntry) => e.item.kind === 'episode' && !!e.item.series_id
  const groupe = (e: HistoryEntry) => e.episodes.length > 1
  const cible = (e: HistoryEntry) => hrefs.films('item', groupe(e) ? e.item.series_id! : e.item.id)
  const titre = (e: HistoryEntry) => e.item.series_title ?? e.item.title
  const heure = (e: HistoryEntry) => new Date(e.seen_at).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' })
  function detail(e: HistoryEntry): string {
    if (groupe(e)) return `${episodesLabel(e.episodes)} · ${e.episodes.length} épisodes`
    if (serie(e)) return [episodeLabel(e.item), e.item.title].filter(Boolean).join(' · ')
    return ['Film', e.item.year, e.item.duration ? shortDuration(e.item.duration) : ''].filter(Boolean).join(' · ')
  }

  async function load(start: number) {
    const mine = ++turn
    loading = true
    try {
      const r = await filmsApi.seen(start)
      if (mine !== turn) return
      items = start ? [...items, ...r.items.filter((i) => !items.some((d) => d.id === i.id))] : r.items
      total = r.total
    } catch (e) { if (mine === turn) notify.error(e, 'L’historique est indisponible') } finally { if (mine === turn) loading = false }
  }

  $effect(() => { void load(0) })
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Historique</h2>
    {#if total > 0}<span class="v-note">{total} {total > 1 ? 'titres vus' : 'titre vu'}</span>{/if}
  </div>
  {#if loading && !items.length}
    <Loader />
  {:else if !days.length}
    <div class="v-empty"><Icon name="film" size={28} /><p class="v-note">Rien pour l’instant.</p></div>
  {:else}
    {#each days as d (d.key)}
      <section class="film-jour" aria-label={d.label}>
        <h3 class="v-titre film-jour-titre">{d.label}</h3>
        <ul class="film-jour-liste">
          {#each d.entries as e (e.key)}
            <li>
              <a class="film-vu" href={cible(e)}>
                <span class="film-vu-art"><FilmPoster id={serie(e) ? e.item.series_id! : e.item.id} title={titre(e)} image={serie(e) || e.item.poster} /></span>
                <span class="film-vu-texte">
                  <strong class="truncate">{titre(e)}</strong>
                  <span class="v-note truncate">{detail(e)}</span>
                </span>
                {#if e.episodes.every((x) => x.available === false)}<span class="v-tag">Indisponible</span>{/if}
                <span class="v-mono v-mute film-vu-heure">{heure(e)}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
    {#if items.length < total}
      <button class="plbls-button plbls-button--contour film-more" disabled={loading} onclick={() => void load(items.length)}>Afficher plus</button>
    {/if}
  {/if}
</div>

<style>
  .film-jour { display: flex; flex-direction: column; gap: 10px; }
  .film-jour-titre { font-size: 15px; color: var(--gris); }
  .film-jour-liste { display: flex; flex-direction: column; gap: 8px; margin: 0; padding: 0; list-style: none; }
  .film-vu { display: flex; align-items: center; gap: 14px; padding: 8px 18px 8px 8px; border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); color: inherit; text-decoration: none; transition: background var(--d) var(--sortie); }
  .film-vu:hover { background: var(--papier-2); }
  .film-vu:focus-visible { outline: none; box-shadow: var(--anneau); }
  .film-vu-art { position: relative; flex: none; width: 44px; aspect-ratio: 2 / 3; overflow: hidden; border-radius: var(--r-xs); }
  .film-vu-art :global(.film-art-title) { display: none; }
  .film-vu-art :global(.film-art--flat) { padding: 0; }
  .film-vu-texte { display: flex; flex-direction: column; flex: 1; min-width: 0; gap: 2px; }
  .film-vu-texte strong { font-size: 14.5px; }
  .film-vu-heure { flex: none; }
</style>
