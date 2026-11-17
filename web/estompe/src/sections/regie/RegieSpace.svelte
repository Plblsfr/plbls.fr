<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import RegieHealth from './RegieHealth.svelte'
  import RegieDownloads from './RegieDownloads.svelte'
  import RegieDeposits from './RegieDeposits.svelte'

  let { section, onmenu }: { section?: string; onmenu: () => void } = $props()
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1>Régie</h1>
</header>
{#if section === 'downloads'}
  <RegieDownloads />
{:else if section === 'deposits'}
  <RegieDeposits />
{:else}
  <RegieHealth />
{/if}
