<script lang="ts">
  import Icon, { type IconName } from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'

  const LIENS: { id: string | undefined; label: string; icon: IconName }[] = [
    { id: undefined, label: 'Santé', icon: 'chart' },
    { id: 'downloads', label: 'Téléchargements', icon: 'telecharge' },
    { id: 'deposits', label: 'Dépôts', icon: 'envoie' },
  ]
  const route = $derived(router.current)
  const section = $derived(route.name === 'regie' ? route.section : undefined)
</script>

<div class="side-head"><h2>Régie</h2></div>
<div class="side-list">
  <p class="v-note regie-sub">Administration des médias</p>
  {#each LIENS as l (l.label)}
    <a class="nav-item" class:active={section === l.id} href={hrefs.regie(l.id)} aria-current={section === l.id ? 'page' : undefined}>
      <Icon name={l.icon} size={18} />
      <span class="grow truncate">{l.label}</span>
    </a>
  {/each}
</div>

<style>
  .regie-sub { margin: 0 0 var(--espace-2) var(--espace-2); }
</style>
