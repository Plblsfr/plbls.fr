<script lang="ts">
  import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
  import { notify } from '../../../../web/estompe/src/lib/ui'
  import { checkGlobalPtt } from './ptt-global'
  import { overlayAccelerator, OVERLAY_POS } from './overlay'

  let capturingOverlay = $state<'overlayKey' | 'msgReplyKey' | 'msgOpenKey' | null>(null)
  async function captureOverlayKey(e: KeyboardEvent) {
    e.preventDefault()
    const key = overlayAccelerator(e)
    const field = capturingOverlay
    if (!key || !field) return
    capturingOverlay = null
    const err = await checkGlobalPtt(key)
    if (err) return notify.info(err)
    prefs[field] = key
    prefs.save()
  }
</script>

<div class="row">
  <select class="plbls-select" aria-label="Quand afficher l'overlay des participants" bind:value={prefs.overlayWhen} onchange={() => prefs.save()}>
    <option value="games">Overlay : seulement sur les jeux</option><option value="always">Overlay : toujours</option><option value="never">Overlay : jamais</option>
  </select>
</div>
{#if prefs.overlayWhen !== 'never'}
  <div class="row">
    <div class="ov-screen" role="radiogroup" aria-label="Emplacement de l'overlay">
      {#each ['tl', 'tr', 'ml', 'mr', 'bl', 'br'] as const as pos (pos)}
        <button class="ov-pos ov-{pos}" class:on={prefs.overlayCorner === pos} role="radio" aria-checked={prefs.overlayCorner === pos} aria-label={OVERLAY_POS[pos]} title={OVERLAY_POS[pos]} onclick={() => { prefs.overlayCorner = pos; prefs.save() }}></button>
      {/each}
    </div>
    <select class="plbls-select" aria-label="Taille de l'overlay" bind:value={prefs.overlaySize} onchange={() => prefs.save()}>
      <option value="sm">Petit</option><option value="md">Moyen</option><option value="lg">Grand</option>
    </select>
    <select class="plbls-select" aria-label="Participants affichés" bind:value={prefs.overlayMode} onchange={() => prefs.save()}>
      <option value="all">Tous les participants</option><option value="speaking">Seulement ceux qui parlent</option>
    </select>
  </div>
  <div class="row">
    <span class="small">Masquer / afficher : <kbd>{prefs.overlayKey.replace(/Key/g, '')}</kbd></span>
    {#if capturingOverlay === 'overlayKey'}
      <input class="plbls-input" readonly placeholder="Appuie sur le raccourci…" onkeydown={captureOverlayKey} onblur={() => (capturingOverlay = null)} {@attach (n) => n.focus()} />
    {:else}
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (capturingOverlay = 'overlayKey')}>Changer</button>
    {/if}
  </div>
  <div class="row">
    <select class="plbls-select" aria-label="Messages affichés dans l'overlay" bind:value={prefs.msgOverlay} onchange={() => prefs.save()}>
      <option value="both">Messages : MP et mentions</option><option value="dm">Messages : MP seulement</option><option value="mention">Messages : mentions seulement</option><option value="off">Messages : rien</option>
    </select>
  </div>
  {#if prefs.msgOverlay !== 'off'}
    <div class="row">
      <label class="small">Durée d'affichage (s) <input class="plbls-input" type="number" min="2" max="60" bind:value={prefs.msgSeconds} onchange={() => { prefs.msgSeconds = Math.min(60, Math.max(2, Math.round(prefs.msgSeconds) || 8)); prefs.save() }} /></label>
    </div>
    <label class="plbls-toggle">
      <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.msgHide} onchange={() => prefs.save()} />
      <span class="plbls-toggle__piste"></span>
      <span class="plbls-toggle__texte">Masquer le contenu des messages dans l'overlay (expéditeur seulement)</span>
    </label>
    {#each [['msgReplyKey', 'Répondre au dernier message'], ['msgOpenKey', 'Ouvrir la conversation']] as const as [field, label] (field)}
      <div class="row">
        <span class="small">{label} : <kbd>{prefs[field].replace(/Key/g, '')}</kbd></span>
        {#if capturingOverlay === field}
          <input class="plbls-input" readonly placeholder="Appuie sur le raccourci…" onkeydown={captureOverlayKey} onblur={() => (capturingOverlay = null)} {@attach (n) => n.focus()} />
        {:else}
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (capturingOverlay = field)}>Changer</button>
        {/if}
      </div>
    {/each}
    <p class="muted small">Les messages suivent tes réglages de notifications (niveaux, aperçu, ne pas déranger) et le mode de l'overlay ci-dessus. Les raccourcis ne sont actifs que pendant l'affichage d'un message.</p>
  {/if}
  <p class="muted small">Le mode « seulement sur les jeux » détecte une application plein écran sans bordure au premier plan ; le plein écran exclusif n'est pas pris en charge. Le raccourci fonctionne aussi en jeu.</p>
{/if}
