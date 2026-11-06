<script lang="ts">
  import { updater } from '../lib/updater.svelte'
</script>

{#if updater.available && !updater.available.force && updater.dismissed !== updater.available.version}
  <div class="update-pill" role="status">
    <span class="small grow">Nouvelle version disponible : Estompe <strong>{updater.available.version}</strong></span>
    <button class="plbls-button plbls-button--compact" class:is-busy={updater.installing} disabled={updater.installing} onclick={() => updater.install()}>
      {updater.installing ? (updater.progress ? `${Math.round(updater.progress * 100)} %` : 'Installation…') : 'Mettre à jour'}
    </button>
    {#if !updater.installing}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (updater.dismissed = updater.available!.version)}>Plus tard</button>{/if}
  </div>
{/if}
