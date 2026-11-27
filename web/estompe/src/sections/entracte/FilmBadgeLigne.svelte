<script lang="ts" module>
  import { filmsApi } from '../../lib/api'
  import type { FilmDetail } from './films'

  const fiches = new Map<string, Promise<FilmDetail | null>>()
  export function ficheBadge(id: string): Promise<FilmDetail | null> {
    if (!filmsApi.base) return Promise.resolve(null)
    let f = fiches.get(id)
    if (!f) {
      f = filmsApi.item(id).catch(() => null)
      fiches.set(id, f)
    }
    return f
  }
</script>

<script lang="ts">
  import FilmPoster from './FilmPoster.svelte'
  import { dateBadge, etiquetteBadge, type BadgeRef } from './badges'

  let { badge }: { badge: BadgeRef } = $props()
  let fiche = $state<FilmDetail | null>(null)

  $effect(() => {
    let actif = true
    fiche = null
    void ficheBadge(badge.title_id).then((f) => { if (actif) fiche = f })
    return () => { actif = false }
  })

  const titre = $derived(fiche?.title ?? 'Titre d’Entracte')
</script>

<span class="film-badge-ligne">
  <span class="film-badge-ligne-art"><FilmPoster id={badge.title_id} title={titre} image={!!fiche?.poster} /></span>
  <span class="film-badge-ligne-texte">
    <b class="truncate">{titre}</b>
    <span class="v-note truncate">{etiquetteBadge(badge)} · {dateBadge(badge.obtained_at)}</span>
  </span>
</span>

<style>
  .film-badge-ligne { display: flex; align-items: center; gap: 12px; flex: 1; min-width: 0; }
  .film-badge-ligne-art { position: relative; flex: none; width: 36px; aspect-ratio: 2 / 3; overflow: hidden; border-radius: var(--r-xs); }
  .film-badge-ligne-art :global(.film-art-title) { display: none; }
  .film-badge-ligne-art :global(.film-art--flat) { padding: 0; }
  .film-badge-ligne-texte { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
</style>
