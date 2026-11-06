<script lang="ts">
  import { tick } from 'svelte'
  import { debug, filterEntries, formatEntry, type Level } from '../lib/debuglog.svelte'
  import { notify } from '../lib/ui'
  import { platform } from '../lib/platform'

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

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={debug.enabled} onchange={(e) => debug.setEnabled(e.currentTarget.checked)} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Mode debug</span>
</label>
<p class="muted small">Enregistre les actions, erreurs et évènements internes (vocal, partage d'écran, réseau). Jamais de jetons, de mots de passe ni de contenu de message. À joindre à un ticket en cas de problème.</p>

{#if Extra && debug.enabled}<Extra />{/if}

<div class="debug-bar">
  <select class="plbls-select" aria-label="Niveau" bind:value={level}>
    <option value="all">Tous les niveaux</option>
    <option value="info">Info</option>
    <option value="warn">Avertissement</option>
    <option value="error">Erreur</option>
  </select>
  <input class="plbls-input" type="search" placeholder="Rechercher" aria-label="Rechercher dans le journal" bind:value={query} />
</div>
<div class="debug-log mono" bind:this={box} role="log" aria-live="off">
  {#each shown as e (e)}
    <div class="debug-{e.l}">{formatEntry(e)}</div>
  {:else}
    <span class="muted">{debug.enabled ? 'Aucune ligne.' : 'Mode debug désactivé : rien n’est enregistré.'}</span>
  {/each}
</div>
<div class="debug-bar">
  <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!debug.entries.length} onclick={copy}>Copier</button>
  <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!debug.entries.length} onclick={exportLog}>Exporter</button>
  <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!debug.entries.length} onclick={() => debug.clear()}>Vider</button>
</div>

<style>
  .debug-bar { display: flex; flex-wrap: wrap; gap: 0.5rem; margin: 0.75rem 0 }
  .debug-log { height: 16rem; overflow: auto; font-size: 0.75rem; letter-spacing: 0; white-space: pre-wrap; word-break: break-word; padding: 0.5rem; border: 1px solid var(--border, currentColor); border-radius: 0.5rem }
  .debug-warn { color: var(--warning, #b7791f) }
  .debug-error { color: var(--danger) }
</style>
