<script lang="ts">
  import Icon from './Icon.svelte'
  import { openReport } from '../blocks/signalements/report.svelte'
  import { updater } from '../lib/updater.svelte'

  const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
</script>

{#if updater.blocked}
  <div class="forcee" role="alertdialog" aria-modal="true" aria-labelledby="forced-title" aria-describedby="forced-desc">
    <div class="boite">
      <div class="tete">
        <span class="v-tag att"><Icon name="alerte" size={12} />Obligatoire</span>
        <h2 id="forced-title" class="v-titre">Mise à jour requise</h2>
        <p id="forced-desc" class="v-note">Cette version d'Estompe ne peut plus être utilisée.{#if updater.available} Installée : <span class="v-mono">{updater.version}</span>, nouvelle : <span class="v-mono">{updater.available.version}</span>.{:else} Installée : <span class="v-mono">{updater.version}</span>.{/if}</p>
        {#if updater.available?.message || updater.refused}<p class="message">{updater.available?.message || updater.refused}</p>{/if}
      </div>
      {#if updater.error || updater.installing}
        <div class="corps">
          {#if updater.error}
            <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{updater.error}</p></div>
          {/if}
          {#if updater.installing}
            <div class="progression">
              <div class="v-note ligne"><span class="grow">Téléchargement</span>{#if updater.progress}<span class="v-mono">{Math.round(updater.progress * 100)} %</span>{/if}</div>
              <progress value={updater.progress || undefined} max="1" aria-label="Téléchargement de la mise à jour"></progress>
            </div>
          {/if}
        </div>
      {/if}
      <div class="pied">
        {#if updater.error}
          <button class="plbls-button plbls-button--discret" onclick={() => openReport()}><Icon name="bug" size={16} />Signaler un bug</button>
        {/if}
        <button class="plbls-button" class:is-busy={updater.installing} disabled={updater.installing} onclick={() => updater.install()}>
          {updater.error ? 'Réessayer' : 'Mettre à jour'}
        </button>
      </div>
    </div>
  </div>
{:else if updater.countdown > 0}
  <div class="compte v-toast" role="alert">
    <span class="v-mono minuteur" aria-hidden="true">{clock(updater.countdown)}</span>
    <span class="grow">Mise à jour obligatoire dans {clock(updater.countdown)}. Termine ce que tu fais, ton brouillon est conservé.</span>
    <button class="plbls-button plbls-button--compact v-rose" onclick={() => updater.install()}>Mettre à jour</button>
  </div>
{/if}

<style>
  .forcee { position: fixed; inset: 0; z-index: var(--z-bloquant); display: grid; place-items: center; padding: 16px; background: color-mix(in srgb, var(--papier-3) 88%, transparent); backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px); }
  .boite { width: min(440px, 100%); max-height: 100%; overflow: auto; display: flex; flex-direction: column; border-radius: var(--r-xl); background: var(--surface); box-shadow: var(--o3); animation: v-monte var(--d-l) var(--sortie); }
  .tete { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; padding: 24px 28px 8px; }
  .tete h2 { font-size: 22px; }
  .tete p { margin: 0; }
  .message { color: var(--encre-2); font-size: 14px; }
  .corps { display: flex; flex-direction: column; gap: 12px; padding: 8px 28px 20px; }
  .corps .plbls-alert { margin: 0; }
  .progression { display: flex; flex-direction: column; gap: 6px; }
  .ligne { display: flex; align-items: center; }
  progress { width: 100%; height: 6px; border: 0; border-radius: var(--r-p); overflow: hidden; background: var(--papier-3); appearance: none; accent-color: var(--rose-fonce); }
  progress::-webkit-progress-bar { background: var(--papier-3); border-radius: var(--r-p); }
  progress::-webkit-progress-value { background: var(--rose-fonce); border-radius: var(--r-p); }
  progress::-moz-progress-bar { background: var(--rose-fonce); border-radius: var(--r-p); }
  .pied { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; margin-top: 12px; padding: 16px 28px; border-top: 1px solid var(--trait); background: var(--surface-2); }
  .compte { position: fixed; left: 50%; top: 16px; z-index: var(--z-notification); width: min(560px, calc(100% - 32px)); transform: translateX(-50%); font-size: 13.5px; }
  .minuteur { flex: none; padding: 4px 8px; border-radius: 6px; background: var(--rose); color: var(--encre); font-weight: 700; }
  @media (prefers-reduced-motion: reduce) {
    .boite { animation: none; }
  }
  @media (pointer: coarse) {
    .pied .plbls-button, .compte .plbls-button { min-height: 44px; }
  }
</style>
