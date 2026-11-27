<script lang="ts">
  import { filmsApi } from '../../lib/api'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import FilmPoster from './FilmPoster.svelte'
  import FilmBadgeCarte from './FilmBadgeCarte.svelte'
  import { afficheBadge, cleBadge, dateBadge, detailsBadge, etiquetteBadge, type FilmBadge, type GenreBadge } from './badges'

  const FILTRES: { id: GenreBadge | ''; label: string }[] = [
    { id: '', label: 'Tous' },
    { id: 'movie', label: 'Films' },
    { id: 'season', label: 'Saisons' },
    { id: 'series', label: 'Séries' },
  ]
  let items = $state<FilmBadge[]>([])
  let loading = $state(true)
  let filtre = $state<GenreBadge | ''>('')
  let ouvert = $state<FilmBadge | null>(null)

  const visibles = $derived(filtre ? items.filter((b) => b.kind === filtre) : items)
  const compte = (id: GenreBadge | '') => (id ? items.filter((b) => b.kind === id).length : items.length)

  $effect(() => {
    let actif = true
    filmsApi.badges().then((r) => { if (actif) items = r.items }, (e) => notify.error(e, 'Les badges sont indisponibles')).finally(() => { if (actif) loading = false })
    return () => { actif = false }
  })
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Badges</h2>
    {#if items.length}
      <div class="v-seg" role="group" aria-label="Type de badge">
        {#each FILTRES as f (f.id)}
          {#if !f.id || compte(f.id)}<button type="button" aria-pressed={filtre === f.id} onclick={() => (filtre = f.id)}>{f.label} · {compte(f.id)}</button>{/if}
        {/each}
      </div>
    {/if}
  </div>
  <p class="v-note film-badges-regle">Un badge se gagne en regardant vraiment : au moins 90 % de la durée lue. L’intro passée avec « Passer l’intro » et la fin passée avec « Épisode suivant » comptent ; avancer à la main, non. Une saison compte quand tous ses épisodes sont regardés, une série quand toutes ses saisons le sont.</p>
  {#if loading && !items.length}
    <Loader />
  {:else if !items.length}
    <div class="v-empty"><Icon name="medaille" size={28} /><p class="v-note">Aucun badge pour l’instant. Termine un film ou une saison pour gagner le premier.</p></div>
  {:else}
    <div class="film-grid">
      {#each visibles as b (cleBadge(b))}
        {@const a = afficheBadge(b)}
        <button type="button" class="film-card film-badge" onclick={() => (ouvert = b)} aria-label={`${etiquetteBadge(b)} : ${b.title}. Partager`}>
          <span class="film-card-art">
            <FilmPoster id={a.id} title={b.title} image={a.image} />
            <span class="film-badge-sceau" aria-hidden="true"><Icon name="ok" size={18} /></span>
          </span>
          <span class="film-badge-etiquette">{etiquetteBadge(b)}</span>
          <strong class="truncate">{b.title}</strong>
          <span class="v-mono v-mute truncate">{detailsBadge(b) || ' '}</span>
          <span class="v-note truncate">le {dateBadge(b.obtained_at)}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

{#if ouvert}<FilmBadgeCarte badge={ouvert} onclose={() => (ouvert = null)} />{/if}

<style>
  .film-badges-regle { max-width: 70ch; margin: 0; }
  .film-badge { border: 0; background: none; padding: 0; text-align: left; font: inherit; color: inherit; cursor: pointer; }
  .film-badge:focus-visible { outline: none; box-shadow: var(--anneau); border-radius: var(--r-l); }
  .film-badge-sceau { position: absolute; right: 8px; bottom: 8px; display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; background: var(--rose-texte); color: var(--surface); box-shadow: var(--o1); }
  .film-badge-etiquette { font-size: 12px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--rose-texte); }
</style>
