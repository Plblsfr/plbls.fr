<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import RegieHealth from './RegieHealth.svelte'

  let { onmenu }: { section?: string; onmenu: () => void } = $props()
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1>Régie</h1>
</header>
<RegieHealth />
