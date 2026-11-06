<script lang="ts">
  let { error, retry }: { error: string | null; retry: () => void } = $props();
</script>

{#if !error}
  <div class="plbls-spinner" role="status" aria-label="Chargement"></div>
{:else}
  <div class="plbls-alert plbls-alert--erreur" role="alert">
    <p class="plbls-alert__titre">Chargement impossible</p>
    <div class="plbls-alert__corps">
      {error}{" "}<button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={retry}>Réessayer</button>
    </div>
  </div>
{/if}
