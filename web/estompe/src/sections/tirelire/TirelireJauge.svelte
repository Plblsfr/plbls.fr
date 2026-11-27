<script lang="ts">
  import { jauge } from './stats'

  let { montant, reference }: { montant: number; reference: number | null } = $props()

  const j = $derived(jauge(montant, reference))
</script>

<span class="tir-jg" aria-hidden="true">
  <span class="tir-jg-plein" style:width="{j.plein}%"></span>
  {#if j.exces > 0}<span class="tir-jg-exces" style:width="{j.exces}%"></span>{/if}
  {#if j.repere !== null}<span class="tir-jg-repere" style:left="{j.repere}%"></span>{/if}
</span>
