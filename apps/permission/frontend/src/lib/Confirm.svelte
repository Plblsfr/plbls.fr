<script lang="ts">
  let pending = $state<{ message: string; confirmLabel: string; resolve: (v: boolean) => void } | null>(null);

  export function ask(message: string, confirmLabel = "Supprimer"): Promise<boolean> {
    return new Promise((resolve) => { pending = { message, confirmLabel, resolve }; });
  }

  const answer = (v: boolean) => { pending?.resolve(v); pending = null; };
</script>

{#if pending}
  <plbls-modal open title="Confirmer" onplbls-close={() => answer(false)}>
    <p>{pending.message}</p>
    <div class="app-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={() => answer(false)}>Annuler</button>
      <button type="button" class="plbls-button plbls-button--danger" onclick={() => answer(true)}>{pending.confirmLabel}</button>
    </div>
  </plbls-modal>
{/if}
