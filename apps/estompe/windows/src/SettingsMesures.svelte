<script lang="ts">
  import { onDestroy } from 'svelte'
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import { notify } from '../../../../web/estompe/src/lib/ui'
  import { invoke } from '../../crates/core/ts/tauri'
  import { SCENARIOS } from './mesures'
  import { arreterSession, demarrerSession, exporterMesures, interrompreScenario, lancerScenario, mesures, scenario } from './mesures.svelte'

  let maintenant = $state(Date.now())
  const horloge = setInterval(() => (maintenant = Date.now()), 1000)
  onDestroy(() => clearInterval(horloge))

  const enCours = $derived(mesures.scenario ? scenario(mesures.scenario.id) : undefined)
  const reste = $derived(mesures.scenario ? Math.max(0, Math.ceil((mesures.scenario.fin - maintenant) / 1000)) : 0)
  const faits = $derived(new Set(mesures.marqueurs.map((m) => m.nom)))

  let sobre = $state<boolean | null>(null)
  invoke<boolean>('webview2_options_get').then((v) => (sobre = v)).catch(() => {})
  async function changerOptions(v: boolean) {
    try {
      await invoke('webview2_options_set', { actives: v })
      sobre = v
      notify.info('Pris en compte au prochain démarrage d’Estompe.')
    } catch (e) { notify.error(e, 'Réglage impossible à enregistrer') }
  }
</script>

<div class="mes">
  <strong>Mesures de performance</strong>
  <p class="v-note">Mémoire et CPU d'Estompe et de tous ses processus WebView2 (overlays à part), fluidité et latences. Rien n'est envoyé : l'export est un fichier à partager si vous le souhaitez, sans contenu de message ni identifiant.</p>
  {#if sobre !== null}
    <label class="plbls-toggle">
      <input class="plbls-toggle__input" type="checkbox" checked={sobre} onchange={(e) => changerOptions(e.currentTarget.checked)} aria-describedby="mes-sobre" />
      <span class="plbls-toggle__piste"></span>
      <span class="plbls-toggle__texte">Options d'économie de mémoire de WebView2</span>
    </label>
    <p class="v-note" id="mes-sobre">Un seul processus de rendu, sans processus de réserve ni cache de navigation. Désactiver pour comparer les mesures ; effet au prochain démarrage.</p>
  {/if}
  <div class="mes-bar">
    {#if mesures.debut === null}
      <button class="plbls-button plbls-button--compact" disabled={mesures.occupe} onclick={demarrerSession}>Démarrer une session</button>
    {:else}
      <button class="plbls-button plbls-button--compact" disabled={mesures.occupe} onclick={arreterSession}>Arrêter la session</button>
    {/if}
    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!mesures.dernier || mesures.debut !== null} onclick={exporterMesures}><Icon name="telecharge" size={16} />Exporter les mesures</button>
  </div>
  {#if mesures.debut !== null}
    {#if enCours}
      <p class="v-note" role="status">En cours : {enCours.titre} ({reste} s). {enCours.aide}</p>
      <div class="mes-bar"><button class="plbls-button plbls-button--compact plbls-button--discret" onclick={interrompreScenario}>Terminer le scénario</button></div>
    {:else}
      <p class="v-note">Scénarios guidés : chacun pose un marqueur, les statistiques sont calculées par scénario.</p>
    {/if}
    <div class="mes-scenarios">
      {#each SCENARIOS as s (s.id)}
        <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!!mesures.scenario} title={s.aide} onclick={() => lancerScenario(s)}>{#if faits.has(s.id)}<Icon name="check" size={14} />{/if}{s.titre}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .mes { display: flex; flex-direction: column; gap: 8px; }
  .mes-bar, .mes-scenarios { display: flex; flex-wrap: wrap; gap: 8px; }
</style>
