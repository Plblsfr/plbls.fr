<script lang="ts">
  import { openReport } from '../blocks/signalements/report.svelte'
  import { updater } from '../lib/updater.svelte'

  const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
</script>

{#if updater.blocked}
  <div class="forced" role="alertdialog" aria-modal="true" aria-labelledby="forced-title">
    <div class="card">
      <h2 id="forced-title">Mise à jour requise</h2>
      <p class="muted">Cette version d'Estompe ne peut plus être utilisée.{updater.available ? ` Installée : ${updater.version}, nouvelle : ${updater.available.version}.` : ` Installée : ${updater.version}.`}</p>
      {#if updater.available?.message || updater.refused}<p>{updater.available?.message || updater.refused}</p>{/if}
      {#if updater.error}
        <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{updater.error}</p></div>
        <button class="plbls-button plbls-button--contour plbls-button--block" onclick={() => openReport()}>Signaler un bug</button>
      {/if}
      {#if updater.installing}
        <progress value={updater.progress || undefined} max="1" aria-label="Téléchargement de la mise à jour"></progress>
      {/if}
      <button class="plbls-button plbls-button--block" class:is-busy={updater.installing} disabled={updater.installing} onclick={() => updater.install()}>
        {updater.error ? 'Réessayer' : 'Mettre à jour'}
      </button>
    </div>
  </div>
{:else if updater.countdown > 0}
  <div class="update-pill floating" role="alert">
    <span class="small grow">Mise à jour obligatoire dans {clock(updater.countdown)}. Termine ce que tu fais, ton brouillon est conservé.</span>
    <button class="plbls-button plbls-button--compact" onclick={() => updater.install()}>Mettre à jour</button>
  </div>
{/if}
