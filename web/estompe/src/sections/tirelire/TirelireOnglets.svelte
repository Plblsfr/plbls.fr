<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { ONGLETS } from './onglets'

  let { actif }: { actif: string | undefined } = $props()
</script>

<nav class="tir-onglets" aria-label="Sections de Tirelire">
  {#each ONGLETS as o (o.label)}
    <a href={hrefs.tirelire(o.id)} aria-current={actif === o.id ? 'page' : undefined}><Icon name={o.icon} size={20} />{o.label}</a>
  {/each}
</nav>
