<script lang="ts">
  import { onMount } from 'svelte'
  import { initials } from '../../../../web/estompe/src/lib/format'
  import { OVERLAY_AVATAR, type OverlayState } from './overlay'

  let ovs = $state<OverlayState>({ size: 'md', items: [] })
  let broken = $state<Record<string, boolean>>({})

  onMount(() => {
    let off = () => {}
    void import('@tauri-apps/api/event').then(async ({ listen, emit }) => {
      off = await listen<OverlayState>('overlay-state', (e) => { ovs = e.payload; broken = {} })
      await emit('overlay-ready')
    })
    return () => off()
  })
  const av = $derived(OVERLAY_AVATAR[ovs.size])
</script>

<svelte:head>
  <style>html, body { background: transparent !important; overflow: hidden; margin: 0; }</style>
</svelte:head>

<div class="ov" style:--av="{av}px">
  {#each ovs.items as i (i.id)}
    <div class="ov-row">
      <span class="ov-av" class:speaking={i.speaking}>
        {#if i.avatar && !broken[i.id]}<img src={i.avatar} alt="" onerror={() => (broken[i.id] = true)} />{:else}{initials(i.pseudo)}{/if}
      </span>
      <span class="ov-name">{i.pseudo}</span>
      {#if i.deaf}<span title="Casque coupé">🔇</span>{:else if i.muted}<span title="Micro coupé">🎙️🚫</span>{/if}
    </div>
  {/each}
</div>

<style>
  .ov { display: flex; flex-direction: column; gap: 8px; font-family: system-ui, sans-serif; user-select: none; }
  .ov-row { display: flex; align-items: center; gap: 10px; }
  .ov-av { width: var(--av); height: var(--av); flex: none; border-radius: 50%; overflow: hidden; display: grid; place-items: center; background: #444; color: #fff; font-weight: 600; box-sizing: border-box; border: 3px solid transparent; }
  .ov-av img { width: 100%; height: 100%; object-fit: cover; }
  .ov-av.speaking { border-color: #4ade80; box-shadow: 0 0 10px #4ade80; }
  .ov-name { color: #fff; background: rgba(0, 0, 0, 0.6); padding: 2px 8px; border-radius: 6px; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
