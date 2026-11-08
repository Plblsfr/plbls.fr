<script lang="ts">
  import Loader from '../Loader.svelte'
  import Icon from '../Icon.svelte'

  let { error, retry }: { error: string | null; retry: () => void } = $props()
</script>

{#if !error}
  <Loader size="sm" text="Chargement…" />
{:else}
  <div class="v-alert dgr" role="alert">
    <Icon name="alerte" size={18} />
    <span class="grow">{error}</span>
    <button class="plbls-button plbls-button--lien plbls-button--compact" onclick={retry}>Réessayer</button>
  </div>
{/if}
