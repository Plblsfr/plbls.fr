<script lang="ts">
  import { platform } from '../lib/platform'
  import { updater } from '../lib/updater.svelte'

  /** Clic sur la version installée (la plateforme peut y cacher quelque chose). */
  let { onversion }: { onversion?: () => void } = $props()
</script>

<p class="muted small" role="presentation" onclick={onversion}>Version installée : {updater.version || '…'}</p>
{#if updater.available}
  <p class="small">Estompe <strong>{updater.available.version}</strong> est disponible.{updater.available.notes ? ` ${updater.available.notes}` : ''}</p>
  <button class="plbls-button plbls-button--compact" class:is-busy={updater.installing} disabled={updater.installing} onclick={() => updater.install()}>
    {updater.installing ? (updater.progress ? `Téléchargement ${Math.round(updater.progress * 100)} %` : 'Installation…') : platform.updates.installLabel}
  </button>
{:else}
  <button class="plbls-button plbls-button--compact plbls-button--contour" class:is-busy={updater.checking} disabled={updater.checking} onclick={() => updater.check(true)}>Rechercher une mise à jour</button>
  {#if !updater.checking && !updater.error && updater.version}<p class="muted small">Tu as la dernière version, ou aucune vérification n'a encore abouti.</p>{/if}
{/if}
{#if updater.error}<p class="small" style="color: var(--danger)">{updater.error}</p>{/if}
