<script lang="ts">
  import type { Quality } from '../blocks/vocal/voice.svelte'

  let { q }: { q: Quality | undefined } = $props()

  const BARS: Record<Quality, number> = { excellent: 3, good: 3, poor: 2, lost: 1, unknown: 0 }
  const LABEL: Record<Quality, string> = { excellent: 'Connexion excellente', good: 'Connexion bonne', poor: 'Connexion médiocre', lost: 'Connexion perdue', unknown: '' }
  const n = $derived(BARS[q ?? 'unknown'])
</script>

{#if q && q !== 'unknown'}
  <span class="quality q-{q}" role="img" aria-label={LABEL[q]} title={LABEL[q]}>
    {#each [1, 2, 3] as i}<i class:on={i <= n}></i>{/each}
  </span>
{/if}
