<script lang="ts">
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { draw } from './draw.svelte'
  import { ranking } from './draw'

  let { channelId }: { channelId: string } = $props()

  const g = $derived(draw.games[channelId])
  const rank = $derived(ranking(g.state.players))
  const steps = $derived([1, 0, 2].filter((i) => rank[i]).map((i) => ({ p: rank[i], place: i + 1 })))
  const rest = $derived(rank.slice(3))
  const isHost = $derived(g.state.host === app.meId)
  const fail = (e: unknown) => notify.error(e, 'Action refusée')
</script>

<div class="end">
  <h2>Fin de la partie</h2>
  <div class="podium">
    {#each steps as { p, place } (p.id)}
      <div class="step" class:first={place === 1} style="--h: {place === 1 ? 200 : place === 2 ? 150 : 110}px">
        <strong>{place}</strong>
        <span>{p.pseudo}</span>
        <small>{p.points} pts</small>
      </div>
    {/each}
  </div>
  <ol class="rest" start="4">
    {#each rest as p (p.id)}<li><span>{p.pseudo}</span><span>{p.points} pts</span></li>{/each}
  </ol>
  <div class="actions">
    {#if isHost}
      <button class="plbls-button" onclick={() => void api.drawStart(channelId).catch(fail)}>Rejouer</button>
      <button class="plbls-button plbls-button--contour" onclick={() => void api.drawStop(channelId).catch(fail)}>Quitter</button>
    {:else}
      <p class="muted">L'hôte peut relancer une partie.</p>
      <button class="plbls-button plbls-button--contour" onclick={() => (g.hidden = true)}>Quitter</button>
    {/if}
  </div>
</div>

<style>
  .end { display: flex; flex-direction: column; align-items: center; gap: var(--espace-4); padding: var(--espace-5) var(--espace-4); overflow: auto; min-height: 0; }
  h2 { font-family: var(--font-titre); font-size: 30px; margin: 0; }
  .podium { display: flex; align-items: flex-end; gap: var(--espace-3); }
  .step { width: min(150px, 28vw); height: var(--h); background: var(--calque); color: var(--noir); border-radius: var(--rayon) var(--rayon) 0 0; display: flex; flex-direction: column; align-items: center; padding-top: 14px; gap: 4px; text-align: center; overflow-wrap: anywhere; }
  .step.first { background: var(--rose); }
  .step strong { font-size: 22px; }
  .step small { font-size: 13px; }
  .rest { list-style: none; margin: 0; padding: 0; width: min(520px, 100%); display: grid; gap: 6px; }
  .rest li { display: flex; justify-content: space-between; padding: 8px 12px; background: var(--surface); box-shadow: 0 0 0 1px var(--trait); border-radius: 8px; }
  .actions { display: flex; gap: var(--espace-3); align-items: center; flex-wrap: wrap; justify-content: center; }
</style>
