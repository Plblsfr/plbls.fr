<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi } from '../../lib/api'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { requestSteps, seasonsLabel, stateLabel, stateTone, type EntracteRequest } from './entracte'

  let { version = 0 }: { version?: number } = $props()

  let items = $state<EntracteRequest[] | null>(null)

  $effect(() => {
    void version
    void entracteApi.mine().then((r) => (items = r.items)).catch((e) => { items = []; notify.error(e, 'Tes demandes sont indisponibles') })
  })
  const date = (s: string) => new Date(s).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long' })
</script>

<aside class="mes">
  <h2 class="v-titre">Mes demandes</h2>
  {#if !items}
    <Loader />
  {:else if !items.length}
    <p class="v-note">Aucune demande pour l’instant. Cherche un titre absent du serveur pour le demander.</p>
  {:else}
    <ul class="mes-liste">
      {#each items as r (r.id)}
        <li class="mes-item">
          <div class="mes-tete">
            <span class="mes-corps"><strong>{r.title}{r.year ? ` (${r.year})` : ''}</strong><span class="v-note">{seasonsLabel(r)} · demandé le {date(r.created_at)}</span></span>
            <span class="v-tag {stateTone(r.state)}">{stateLabel(r.state)}</span>
          </div>
          <div class="mes-jauge" aria-hidden="true">{#each requestSteps(r.state) as e, i (i)}<span class={e}></span>{/each}</div>
          {#if r.state === 'refusee' && r.reason}<span class="v-note mes-motif">Motif : {r.reason}</span>{/if}
          {#if r.state === 'disponible' && r.title_id}<a class="plbls-button plbls-button--compact mes-voir" href={hrefs.films('item', r.title_id)}><Icon name="lecture" size={14} /> Regarder</a>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .mes { flex: 1 1 300px; min-width: 0; align-self: flex-start; display: flex; flex-direction: column; gap: var(--espace-3); padding: var(--espace-5); border-radius: var(--r-xl); background: var(--papier-2); }
  .mes h2 { font-size: 22px; }
  .mes > p { margin: 0; }
  .mes-liste { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .mes-item { display: flex; flex-direction: column; gap: var(--espace-2); padding: var(--espace-3) var(--espace-4); border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .mes-tete { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--espace-2); }
  .mes-corps { display: flex; flex-direction: column; min-width: 0; }
  .mes-jauge { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 4px; }
  .mes-jauge span { height: 4px; border-radius: var(--r-p); background: var(--papier-3); }
  .mes-jauge .fait { background: var(--encre); }
  .mes-jauge .ok { background: var(--ok); }
  .mes-jauge .refus { background: var(--danger); }
  .mes-motif { color: var(--danger); }
  .mes-voir { align-self: flex-start; text-decoration: none; }
</style>
