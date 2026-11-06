<script lang="ts">
  import { onMount } from 'svelte'
  import type { BobynOverlayState } from './overlay'

  let state = $state<BobynOverlayState | null>(null)

  onMount(() => {
    let off = () => {}
    void import('@tauri-apps/api/event').then(async ({ listen, emit }) => {
      off = await listen<BobynOverlayState | null>('bobyn-overlay-state', (e) => (state = e.payload))
      await emit('bobyn-overlay-ready')
    })
    return () => off()
  })
</script>

<svelte:head>
  <style>html, body { background: transparent !important; overflow: hidden; margin: 0; }</style>
</svelte:head>

{#if state}
  <div class="bb">
    <span class="dot {state.dot}"></span>
    <span class="txt">{state.text}</span>
  </div>
{/if}

<style>
  .bb { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; box-sizing: border-box; padding: 4px 10px; border-radius: 6px; background: rgba(0, 0, 0, 0.6); color: #fff; font: 13px system-ui, sans-serif; user-select: none; }
  .txt { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dot { width: 9px; height: 9px; flex: none; border-radius: 50%; }
  .rec { background: #ef4444; }
  .pause { background: #f59e0b; }
  .error { background: #a3a3a3; }
  .saved { background: #4ade80; }
  .marker { background: #60a5fa; }
</style>
