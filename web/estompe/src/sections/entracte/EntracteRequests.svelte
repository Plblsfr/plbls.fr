<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import EntractePoster from './EntractePoster.svelte'
  import { entracteApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { seasonsLabel, stateLabel, stateTone, type EntracteRequest } from './entracte'

  let items = $state<EntracteRequest[] | null>(null)

  $effect(() => {
    void entracteApi.mine().then((r) => (items = r.items)).catch((e) => { items = []; notify.error(e, 'Tes demandes sont indisponibles') })
  })
  const date = (s: string) => new Date(s).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' })
</script>

<div class="film-page">
  <div class="film-filters"><h2 class="v-titre film-filters-title">Mes demandes</h2><a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.films('search')}><Icon name="cherche" size={14} /> Demander un titre</a></div>
  {#if !items}
    <Loader />
  {:else if !items.length}
    <div class="v-empty"><Icon name="bulle" size={28} /><p class="v-note">Aucune demande pour l’instant.</p></div>
  {:else}
    <ul class="req-list">
      {#each items as r (r.id)}
        <li class="req-item">
          <span class="req-art"><EntractePoster url={r.poster_url} title={r.title} seed={r.id} /></span>
          <span class="grow req-body">
            <strong>{r.title}{r.year ? ` (${r.year})` : ''}</strong>
            <span class="v-note">{seasonsLabel(r)} · demandé le {date(r.created_at)}</span>
            {#if r.state === 'refusee' && r.reason}<span class="v-note req-reason">Motif : {r.reason}</span>{/if}
          </span>
          <span class="v-tag {stateTone(r.state)}">{stateLabel(r.state)}</span>
          {#if r.state === 'disponible' && r.title_id}<a class="plbls-button plbls-button--compact" href={hrefs.films('item', r.title_id)}><Icon name="lecture" size={14} /> Voir</a>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .req-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .req-item { display: flex; align-items: center; gap: var(--espace-3); padding: var(--espace-2); border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); }
  .req-art { flex: none; width: 48px; aspect-ratio: 2 / 3; overflow: hidden; border-radius: var(--r-petit, 6px); }
  .req-body { display: flex; flex-direction: column; min-width: 0; }
  .req-reason { color: var(--danger, inherit); }
</style>
