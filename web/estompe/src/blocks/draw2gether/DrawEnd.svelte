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
        <span class="who"><span class="name">{p.pseudo}</span><small class="v-mono">{p.points} pts</small></span>
        <strong>{place}</strong>
      </div>
    {/each}
  </div>
  <ol class="rest" start="4">
    {#each rest as p (p.id)}<li><span>{p.pseudo}</span><span class="v-mono">{p.points} pts</span></li>{/each}
  </ol>
  <div class="actions">
    {#if isHost}
      <button class="plbls-button" onclick={() => void api.drawStart(channelId).catch(fail)}>Rejouer</button>
      <button class="plbls-button plbls-button--contour" onclick={() => void api.drawStop(channelId).catch(fail)}>Quitter</button>
    {:else}
      <p class="v-note">L'hôte peut relancer une partie.</p>
      <button class="plbls-button plbls-button--contour" onclick={() => (g.hidden = true)}>Quitter</button>
    {/if}
  </div>
</div>

<style>
  .end { display: flex; flex-direction: column; align-items: center; gap: 18px; padding: 24px 16px; overflow: auto; min-height: 0; }
  h2 { font-family: var(--f-titre); font-size: 28px; font-weight: 700; letter-spacing: -0.03em; margin: 0; }
  .podium { display: flex; align-items: flex-end; gap: 10px; }
  .step { display: flex; flex-direction: column; align-items: center; gap: 6px; width: min(140px, 28vw); text-align: center; }
  .step strong { display: flex; align-items: center; justify-content: center; width: 100%; height: calc(var(--h) - 60px); border-radius: 12px 12px 0 0; background: var(--papier-2); font-family: var(--f-titre); font-size: 24px; font-weight: 800; }
  .step.first strong { background: var(--rose); color: var(--encre); font-size: 30px; }
  .step:not(.first):nth-child(1) strong { background: var(--papier-3); }
  .who { display: grid; gap: 2px; min-width: 0; }
  .name { font-weight: 600; overflow-wrap: anywhere; }
  .who small { color: var(--gris); }
  .rest { list-style: none; margin: 0; padding: 0; width: min(520px, 100%); display: grid; gap: 4px; }
  .rest li { display: flex; justify-content: space-between; padding: 8px 12px; background: var(--papier-2); border-radius: var(--r-s); font-size: 14px; }
  .actions { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; justify-content: center; }
</style>
