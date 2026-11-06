<script lang="ts">
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import Replays from './Replays.svelte'
  import ReplaySends from './ReplaySends.svelte'
  import SettingsBobyn from './SettingsBobyn.svelte'
  import { hrefs } from '../../../../web/estompe/src/lib/route.svelte'

  let { section, onmenu }: { section?: string; onmenu: () => void } = $props()
  const TABS = [['replays', 'Replays'], ['settings', 'Réglages']] as const
  const current = $derived(section === 'settings' ? 'settings' : 'replays')
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1><span class="muted">Bobyn (alpha) ›</span> {TABS.find((t) => t[0] === current)![1]}</h1>
</header>
<div class="bb-page">
  <div class="row tabs">
    {#each TABS as [id, label] (id)}
      <a class="plbls-button plbls-button--compact" class:plbls-button--contour={current !== id} href={hrefs.space('bobyn', id)}>{label}</a>
    {/each}
  </div>
  <ReplaySends />
  {#if current === 'settings'}<SettingsBobyn />{:else}<Replays />{/if}
</div>

<style>
  .bb-page { padding: var(--espace-4); overflow: auto; flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 0.75rem; }
  .tabs { gap: 0.5rem; flex-shrink: 0; }
</style>
