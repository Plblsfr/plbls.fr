<script lang="ts">
  import Icon from './Icon.svelte'
  import { updater } from '../lib/updater.svelte'
</script>

{#if updater.available && !updater.available.force && updater.dismissed !== updater.available.version}
  <div class="maj" role="status">
    <span class="maj-ico" aria-hidden="true"><Icon name="telecharge" size={16} /></span>
    <span class="maj-t">Nouvelle version disponible : Estompe <strong>{updater.available.version}</strong></span>
    <span class="maj-actions">
      <button class="v-btn sm" class:is-busy={updater.installing} disabled={updater.installing} onclick={() => updater.install()}>
        {updater.installing ? (updater.progress ? `${Math.round(updater.progress * 100)} %` : 'Installation…') : 'Mettre à jour'}
      </button>
      {#if !updater.installing}<button class="v-btn sm ghost" onclick={() => (updater.dismissed = updater.available!.version)}>Plus tard</button>{/if}
    </span>
    {#if updater.installing && updater.progress}<span class="v-prog maj-prog" aria-hidden="true"><i style:width="{Math.round(updater.progress * 100)}%"></i></span>{/if}
  </div>
{/if}

<style>
  .maj { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: var(--espace-2) 10px; padding: 10px 12px; border-radius: var(--r); background: var(--rose); color: var(--encre); font-size: 13px; line-height: 1.35; }
  .maj-ico { display: grid; place-items: center; width: 28px; height: 28px; border-radius: var(--r-s); background: var(--encre); color: var(--rose); }
  .maj-actions { grid-column: 1 / -1; display: flex; gap: var(--espace-1); }
  .maj-actions .v-btn:first-child { flex: 1; }
  .maj-prog { grid-column: 1 / -1; height: 4px; background: color-mix(in srgb, var(--encre) 14%, transparent); }
  @media (pointer: coarse) { .maj-actions .v-btn { min-height: 44px; } }
</style>
