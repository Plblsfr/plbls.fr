<script lang="ts">
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import DrawEnd from './DrawEnd.svelte'
  import DrawLobby from './DrawLobby.svelte'
  import DrawPlay from './DrawPlay.svelte'
  import { draw } from './draw.svelte'

  let { channelId, serverId }: { channelId: string; serverId: string } = $props()

  const g = $derived(draw.games[channelId])
  const joined = $derived(!!g?.state.players.some((p) => p.id === app.meId))
</script>

{#if g}
  <div class="d2g">
    {#if !joined}
      <div class="join v-alert">
        <span class="grow">Tu ne fais pas partie de la partie.</span>
        <button class="plbls-button plbls-button--compact v-rose" onclick={() => void api.drawJoin(channelId).catch((e) => notify.error(e, 'Impossible de rejoindre'))}>Rejoindre</button>
      </div>
    {/if}
    {#if g.state.phase === 'lobby'}<DrawLobby {channelId} {serverId} />
    {:else if g.state.phase === 'ended'}<DrawEnd {channelId} />
    {:else}<DrawPlay {channelId} />{/if}
  </div>
{/if}

<style>
  .d2g { display: flex; flex-direction: column; min-height: 0; overflow: auto; background: var(--surface); border-radius: var(--r-xl); box-shadow: var(--o1); }
  .join { align-items: center; margin: 12px 12px 0; padding: 10px 14px; }
  .grow { flex: 1; min-width: 0; }
</style>
