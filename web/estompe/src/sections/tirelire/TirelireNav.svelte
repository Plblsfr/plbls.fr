<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { ONGLETS, ongletActif, vueDe } from './onglets'

  const route = $derived(router.current)
  const vue = $derived(route.name === 'tirelire' ? vueDe(route.section, route.id) : 'accueil')
  const actif = $derived(ongletActif(vue))
</script>

<div class="side-head"><h2>Tirelire</h2></div>
<nav class="side-list" aria-label="Tirelire">
  {#each ONGLETS as o (o.label)}
    {@const ici = vue !== 'import' && actif === o.id}
    <a class="nav-item" class:active={ici} href={hrefs.tirelire(o.id)} aria-current={ici ? 'page' : undefined}>
      <Icon name={o.icon} size={18} />
      <span class="grow truncate">{o.long}</span>
    </a>
  {/each}
  <p class="tir-nav-titre">Outils</p>
  <a class="nav-item" class:active={vue === 'import'} href={hrefs.tirelire('import')} aria-current={vue === 'import' ? 'page' : undefined}>
    <Icon name="envoie" size={18} />
    <span class="grow truncate">Import</span>
  </a>
</nav>
