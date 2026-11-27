<script lang="ts">
  import { teinte, type Categorie } from './tirelire'

  let { categorie, fort = false }: { categorie: Categorie | null; fort?: boolean } = $props()
</script>

{#if categorie}
  <span class="tir-cat" class:tir-cat--fort={fort}><span class="tir-pt tir-pt--{teinte(categorie)}" aria-hidden="true"></span><span>{categorie.name}</span></span>
{:else}
  <span class="tir-etq tir-etq--rose">à classer</span>
{/if}
