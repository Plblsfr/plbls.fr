<script lang="ts">
  import { etincelle } from './stats'

  let { valeurs, largeur = 64, hauteur = 24 }: { valeurs: number[]; largeur?: number; hauteur?: number } = $props()

  const t = $derived(etincelle(valeurs, largeur, hauteur))
</script>

{#if t}
  <svg class="tir-etincelle" width={largeur} height={hauteur} viewBox="0 0 {largeur} {hauteur}" aria-hidden="true">
    <path d={t.chemin} />
    <circle cx={t.x} cy={t.y} r="2.5" />
  </svg>
{/if}
