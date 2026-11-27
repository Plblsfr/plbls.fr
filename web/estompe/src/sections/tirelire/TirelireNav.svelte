<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { estOutil, ONGLETS, ongletActif, vueDe } from './onglets'
  import { tirelire } from './tirelire.svelte'

  const route = $derived(router.current)
  const vue = $derived(route.name === 'tirelire' ? vueDe(route.section, route.id) : 'accueil')
  const actif = $derived(ongletActif(vue))
</script>

<div class="side-head"><h2>Tirelire</h2></div>
<nav class="side-list" aria-label="Tirelire">
  {#each ONGLETS as o (o.label)}
    {@const ici = !estOutil(vue) && actif === o.id}
    <a class="nav-item" class:active={ici} href={hrefs.tirelire(o.id)} aria-current={ici ? 'page' : undefined}>
      <Icon name={o.icon} size={18} />
      <span class="grow truncate">{o.long}</span>
    </a>
  {/each}
  <p class="tir-nav-titre">Outils</p>
  <a class="nav-item" class:active={vue === 'classer'} href={hrefs.tirelire('a-classer')} aria-current={vue === 'classer' ? 'page' : undefined}>
    <Icon name="boite" size={18} />
    <span class="grow truncate">À classer</span>
    {#if tirelire.aClasser}<span class="pip" aria-label="{tirelire.aClasser} à classer">{tirelire.aClasser > 99 ? '99+' : tirelire.aClasser}</span>{/if}
  </a>
  <a class="nav-item" class:active={vue === 'categories'} href={hrefs.tirelire('categories')} aria-current={vue === 'categories' ? 'page' : undefined}>
    <Icon name="curseurs" size={18} />
    <span class="grow truncate">Catégories et règles</span>
  </a>
  <a class="nav-item" class:active={vue === 'import'} href={hrefs.tirelire('import')} aria-current={vue === 'import' ? 'page' : undefined}>
    <Icon name="envoie" size={18} />
    <span class="grow truncate">Import</span>
  </a>
</nav>
