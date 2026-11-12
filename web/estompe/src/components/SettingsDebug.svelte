<script lang="ts">
  import { tick } from 'svelte'
  import { debug, filterEntries, formatEntry, type Level } from '../lib/debuglog.svelte'
  import { notify } from '../lib/ui'
  import { platform } from '../lib/platform'
  import Icon from './Icon.svelte'

  const Extra = platform.slots.settingsDebug

  let level = $state<Level | 'all'>('all')
  let query = $state('')
  let box = $state<HTMLElement>()
  const shown = $derived(filterEntries(debug.entries, level, query))
  const all = () => debug.entries.map(formatEntry).join('\n')

  // Suit les nouvelles lignes, sauf si on a remonté le journal pour lire.
  $effect(() => {
    void shown.length
    const el = box
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 40) void tick().then(() => (el.scrollTop = el.scrollHeight))
  })

  $effect(() => {
    if (!debug.enabled || !platform.debugStore) return
    void debug.refresh()
    const suivi = setInterval(() => void debug.refresh(), 3000)
    return () => clearInterval(suivi)
  })

  async function copy() {
    try { await navigator.clipboard.writeText(all()); notify.info('Journal copié.') } catch { notify.info('Copie impossible.') }
  }
  function exportLog() {
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([all()], { type: 'text/plain' }))
    a.download = `estompe-debug-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-')}.log`
    a.click()
    URL.revokeObjectURL(a.href)
  }
</script>

<section class="p-section">
  <div class="p-carte p-carte-pleine">
    <label class="plbls-toggle">
      <input class="plbls-toggle__input" type="checkbox" checked={debug.enabled} onchange={(e) => debug.setEnabled(e.currentTarget.checked)} aria-describedby="debug-aide" />
      <span class="plbls-toggle__piste"></span>
      <span class="plbls-toggle__texte"><strong>Mode debug</strong></span>
    </label>
    <p class="v-note" id="debug-aide">Enregistre les actions, erreurs et évènements internes (vocal, partage d'écran, réseau). Jamais de jetons, de mots de passe ni de contenu de message. À joindre à un ticket en cas de problème.</p>

    {#if Extra && debug.enabled}<div class="debug-extra"><Extra /></div>{/if}

    <div class="debug-bar">
      <select class="plbls-select" aria-label="Niveau" bind:value={level}>
        <option value="all">Tous les niveaux</option>
        <option value="info">Info</option>
        <option value="warn">Avertissement</option>
        <option value="error">Erreur</option>
      </select>
      <input class="plbls-input debug-cherche" type="search" placeholder="Rechercher" aria-label="Rechercher dans le journal" bind:value={query} />
    </div>
    <div class="debug-log v-mono" bind:this={box} role="log" aria-live="off">
      {#each shown as e (e)}
        <div class="debug-{e.l}">{formatEntry(e)}</div>
      {:else}
        <span class="debug-vide">{debug.enabled ? 'Aucune ligne.' : 'Mode debug désactivé : rien n’est enregistré.'}</span>
      {/each}
    </div>
    <div class="debug-bar">
      <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!debug.entries.length} onclick={copy}><Icon name="copie" size={16} />Copier</button>
      <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!debug.entries.length} onclick={exportLog}><Icon name="telecharge" size={16} />Exporter</button>
      <span class="debug-pousse"></span>
      <button class="plbls-button plbls-button--compact plbls-button--discret debug-vider" disabled={!debug.entries.length} onclick={() => debug.clear()}>Vider</button>
    </div>
  </div>
</section>

<style>
  .debug-extra { display: flex; flex-direction: column; }
  .debug-bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .debug-bar .plbls-select { flex: 0 1 200px; }
  .debug-cherche { flex: 1 1 200px; min-width: 0; }
  .debug-pousse { flex: 1; }
  .debug-vider { color: var(--danger); }
  .debug-log { height: 16rem; overflow: auto; padding: 12px; border-radius: var(--r); background: var(--encre); color: var(--papier); font-size: 11.5px; line-height: 1.6; letter-spacing: 0; white-space: pre-wrap; word-break: break-word; }
  .debug-warn { color: var(--attention-fond); font-weight: 600; }
  .debug-error { color: var(--danger-fond); font-weight: 700; }
  .debug-vide { opacity: 0.75; }
</style>
