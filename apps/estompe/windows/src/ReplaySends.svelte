<script lang="ts">
  import { sends } from './replaySend.svelte'

  const STEP = { compress: 'Réencodage', upload: 'Envoi', send: 'Publication', done: 'Envoyé', error: 'Échec' } as const
</script>

{#each sends.jobs as j (j.id)}
  <div class="row rs-job" role="status">
    <span class="truncate" title={j.name}>{j.name} → {j.to}</span>
    <span class="muted small">{STEP[j.step]}{j.retry ? ` (nouvelle tentative ${j.retry})` : ''}</span>
    {#if j.step === 'compress' || j.step === 'upload'}<progress max="1" value={j.pct} aria-label="Progression"></progress>{/if}
    {#if j.step === 'error'}
      <span class="small" style="color: var(--danger)">{j.error}</span>
      <button class="plbls-button plbls-button--compact" onclick={() => sends.retry(j.id)}>Réessayer</button>
    {/if}
    {#if j.step !== 'done'}
      <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => sends.cancel(j.id)}>{j.step === 'error' ? 'Abandonner' : 'Annuler'}</button>
    {/if}
  </div>
{/each}

<style>
  .rs-job { gap: 0.75rem; align-items: center; flex-wrap: wrap; flex-shrink: 0; }
  .rs-job .truncate { max-width: 40ch; }
  progress { width: 160px; }
</style>
