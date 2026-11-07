<script lang="ts">
  let { size = 'md', label = 'Chargement', text = '' }: { size?: 'sm' | 'md' | 'lg'; label?: string; text?: string } = $props()
</script>

<span class="loader loader-{size}" role="status" aria-label={text || label}>
  <svg viewBox="0 0 128 128" aria-hidden="true">
    <path class="w w1" pathLength="1" d="M20 56C36 36 52 36 64 56S92 76 108 56" />
    <path class="w w2" pathLength="1" d="M20 76C36 56 52 56 64 76S92 96 108 76" />
    <path class="w w3" pathLength="1" d="M20 96C36 76 52 76 64 96S92 116 108 96" />
  </svg>
  {#if text}<span class="loader-text muted">{text}</span>{/if}
</span>

<style>
  .loader { display: inline-flex; flex-direction: column; align-items: center; gap: 8px; opacity: 0; animation: loader-in 0s linear 300ms forwards; } /* évite le clignotement des chargements rapides */
  svg { width: 40px; height: 40px; fill: none; stroke-width: 10; stroke-linecap: round; }
  .loader-sm svg { width: 20px; height: 20px; }
  .loader-lg svg { width: 96px; height: 96px; }
  .w { stroke-dasharray: 1; stroke-dashoffset: 1; animation: loader-wave 1.8s ease-in-out infinite; }
  .w1 { stroke: var(--encre); }
  .w2 { stroke: var(--rose-fonce); animation-delay: .15s; }
  .w3 { stroke: var(--rose-fonce); animation-delay: .3s; }
  @keyframes loader-in { to { opacity: 1; } }
  @keyframes loader-wave { 0% { stroke-dashoffset: 1; opacity: 1; } 50% { stroke-dashoffset: 0; opacity: 1; } 100% { stroke-dashoffset: 0; opacity: 0; } }
  @media (prefers-reduced-motion: reduce) {
    .w { stroke-dashoffset: 0; animation: loader-pulse 2s ease-in-out infinite; }
    @keyframes loader-pulse { 50% { opacity: .75; } }
  }
</style>
