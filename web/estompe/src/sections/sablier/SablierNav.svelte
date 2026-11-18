<script lang="ts">
  import Icon, { type IconName } from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { sablier } from './sablier.svelte'

  const route = $derived(router.current)
  const section = $derived(route.name === 'stats' ? route.section : undefined)
  const liens = $derived<{ id: string | undefined; label: string; icon: IconName; nouveau: string }[]>([
    { id: undefined, label: 'Jeu et trophées', icon: 'trophee', nouveau: sablier.nonVus.trophies > 0 ? 'Nouveau trophée' : '' },
    { id: 'recaps', label: 'Récaps', icon: 'histoire', nouveau: sablier.nonVus.recaps > 0 ? 'Nouveau récap' : '' },
    { id: 'partage', label: 'Partage et données', icon: 'curseurs', nouveau: '' },
    ...(app.sablierMe?.permissions.admin ? [{ id: 'admin', label: 'Administration', icon: 'bouclier' as IconName, nouveau: '' }] : []),
  ])
  const actif = (id: string | undefined) => (id === undefined ? !liens.some((l) => l.id !== undefined && l.id === section) : section === id)
</script>

<div class="side-head"><h2>Stats</h2></div>
<nav class="side-list" aria-label="Stats">
  {#each liens as l (l.label)}
    <a class="nav-item" class:active={actif(l.id)} href={hrefs.stats(l.id)} aria-current={actif(l.id) ? 'page' : undefined}>
      <Icon name={l.icon} size={18} />
      <span class="grow truncate">{l.label}</span>
      {#if l.nouveau}<span class="sab-pip" role="img" aria-label={l.nouveau}></span>{/if}
    </a>
  {/each}
</nav>
