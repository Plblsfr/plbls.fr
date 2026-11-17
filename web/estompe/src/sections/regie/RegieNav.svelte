<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'

  const route = $derived(router.current)
  const section = $derived(route.name === 'regie' ? route.section : undefined)
</script>

<div class="side-head"><h2>Régie</h2></div>
<div class="side-list">
  <p class="v-note regie-sub">Administration des médias</p>
  <a class="nav-item" class:active={!section} href={hrefs.regie()} aria-current={!section ? 'page' : undefined}>
    <Icon name="chart" size={18} />
    <span class="grow truncate">Santé</span>
  </a>
</div>

<style>
  .regie-sub { margin: 0 0 var(--espace-2) var(--espace-2); }
</style>
