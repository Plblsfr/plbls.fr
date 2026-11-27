<script lang="ts">
  import { ecartCourt, FLECHES, sensEcart, type Ecart } from './stats'

  let { ecart, bon = 'baisse', texte = '' }: { ecart: Ecart | null; bon?: 'hausse' | 'baisse'; texte?: string } = $props()

  const sens = $derived(ecart ? sensEcart(ecart) : 'egal')
</script>

{#if ecart}
  <span class="tir-ec" class:tir-ec--mal={sens !== 'egal' && sens !== bon} class:tir-ec--bien={sens !== 'egal' && sens === bon}><span aria-hidden="true">{FLECHES[sens]}</span>{texte || ecartCourt(ecart).replace(/^=\s*/, '')}</span>
{:else}
  <span class="tir-ec tir-ec--vide" title="Pas de point de comparaison"><span class="tir-sr">pas de comparaison</span><span aria-hidden="true">—</span></span>
{/if}
