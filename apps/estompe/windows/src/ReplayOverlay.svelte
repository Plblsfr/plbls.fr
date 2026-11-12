<script lang="ts">
  import { onMount } from 'svelte'
  import { TAILLE_OVERLAY, type EtatOverlay } from './replay'

  let st = $state<EtatOverlay>({ indicateur: false, messages: [] })

  onMount(() => {
    let off = () => {}
    void import('@tauri-apps/api/event').then(async ({ listen, emit }) => {
      off = await listen<EtatOverlay>('replay-overlay-state', (e) => { st = e.payload })
      await emit('replay-overlay-ready')
    })
    return () => off()
  })
</script>

<svelte:head>
  <style>html, body { background: transparent !important; overflow: hidden; margin: 0; }</style>
</svelte:head>

<div class="ro" style:--ind="{TAILLE_OVERLAY.indicateur}px" style:--ligne="{TAILLE_OVERLAY.ligne}px" style:--ecart="{TAILLE_OVERLAY.ecart}px">
  {#if st.indicateur}<div class="ro-ind" role="status"><span class="ro-pt" aria-hidden="true"></span>Replay</div>{/if}
  {#each st.messages as m (m.id)}
    <div class="ro-msg ro-{m.genre}" role={m.genre === 'erreur' ? 'alert' : 'status'}><span class="ro-pt" aria-hidden="true"></span><span class="ro-txt">{m.texte}</span></div>
  {/each}
</div>

<style>
  .ro { display: flex; flex-direction: column; align-items: flex-end; gap: var(--ecart); font-family: system-ui, sans-serif; color: #fff; user-select: none; }
  .ro-ind { height: var(--ind); box-sizing: border-box; display: inline-flex; align-items: center; gap: 6px; padding: 0 10px; border-radius: 999px; background: rgba(0, 0, 0, 0.45); font-size: 12px; opacity: 0.8; }
  .ro-msg { width: 100%; height: var(--ligne); box-sizing: border-box; display: flex; align-items: center; gap: 10px; padding: 0 12px; border-radius: 10px; background: rgba(0, 0, 0, 0.78); font-size: 13px; line-height: 1.25; }
  .ro-txt { min-width: 0; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .ro-pt { flex: none; width: 8px; height: 8px; border-radius: 50%; background: #ef4444; }
  .ro-ok .ro-pt { background: #4ade80; }
  .ro-info .ro-pt { background: #facc15; }
</style>
