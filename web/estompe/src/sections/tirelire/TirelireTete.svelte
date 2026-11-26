<script lang="ts">
  import type { Snippet } from 'svelte'
  import Icon from '../../components/Icon.svelte'

  let { titre, sur = '', sous = '', retour = '', retourLabel = 'Retour', onmenu, children }: { titre: string; sur?: string; sous?: string; retour?: string; retourLabel?: string; onmenu?: () => void; children?: Snippet } = $props()
</script>

<header class="tir-tete" class:tir-tete--retour={!!retour}>
  {#if retour}
    <a class="tir-ib" href={retour} aria-label={retourLabel}><Icon name="gauche" size={20} /></a>
  {:else if onmenu}
    <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  {/if}
  <div class="tir-col tir-col--colle tir-grandit">
    {#if sur}<span class="tir-note truncate">{sur}</span>{/if}
    <h1>{titre}</h1>
    {#if sous}<span class="tir-note truncate">{sous}</span>{/if}
  </div>
  {@render children?.()}
</header>
