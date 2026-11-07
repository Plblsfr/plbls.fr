<script lang="ts">
  import { prefs } from '../lib/prefs.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'

  const o = prefs.camOverlay
  const changed = () => { prefs.save(); void voice.applyOverlay() }
  const live = () => prefs.save()
  const corner = (x: number, y: number) => { o.x = x; o.y = y; prefs.save() }
</script>

<h3 class="set-title">Incrustation caméra <span class="v-tag encre">ALPHA</span></h3>
<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" bind:checked={o.enabled} onchange={changed} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Incruster ma caméra dans mon partage d'écran</span>
</label>
{#if o.enabled}
  <p class="v-note">Calculée sur ton appareil avant l'envoi : les spectateurs la voient sans rien régler. Active quand la caméra et le partage (sélecteur du système) sont en cours.</p>
  {#await import('./CameraOverlayPreview.svelte') then { default: Preview }}<Preview />{/await}
  <div class="plbls-field">
    <span class="plbls-field__label">Position (ou glisser dans l'aperçu)</span>
    <div class="corners">
      {#each [[0, 0, 'Haut gauche'], [1, 0, 'Haut droite'], [0, 1, 'Bas gauche'], [1, 1, 'Bas droite']] as const as [x, y, label] (label)}
        <button class="v-chip" aria-pressed={o.x === x && o.y === y} onclick={() => corner(x, y)}>{label}</button>
      {/each}
    </div>
  </div>
  <div class="ov-grid">
    <div class="plbls-field ov-range">
      <label class="plbls-field__label" for="ovsize"><span>Taille :</span> <span class="v-mono">{Math.round(o.size * 100)} % de la largeur</span></label>
      <input id="ovsize" type="range" min="0.1" max="0.5" step="0.01" bind:value={o.size} onchange={live} />
    </div>
    <div class="plbls-field ov-range">
      <label class="plbls-field__label" for="ovmargin"><span>Marge :</span> <span class="v-mono">{o.margin} px</span></label>
      <input id="ovmargin" type="range" min="0" max="120" step="4" bind:value={o.margin} onchange={live} />
    </div>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="ovshape">Forme</label>
    <select id="ovshape" class="plbls-select" bind:value={o.shape} onchange={live}>
      <option value="rect">Rectangle arrondi</option>
      <option value="circle">Cercle</option>
      <option value="free" disabled={prefs.cameraFx !== 'cutout'}>Détourage libre (avec « Fond supprimé »)</option>
    </select>
  </div>
  <div class="ov-toggles">
    {#each [['border', 'Bordure'], ['shadow', 'Ombre'], ['mirror', 'Miroir']] as const as [k, label] (k)}
      <label class="plbls-toggle">
        <input class="plbls-toggle__input" type="checkbox" bind:checked={o[k]} onchange={live} />
        <span class="plbls-toggle__piste"></span>
        <span class="plbls-toggle__texte">{label}</span>
      </label>
    {/each}
  </div>
{/if}

<style>
  .set-title { display: flex; align-items: center; gap: 10px; }
  .corners { display: flex; flex-wrap: wrap; gap: 6px; }
  .ov-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 16px; }
  .ov-range label { display: flex; justify-content: space-between; gap: 8px; }
  .ov-range input { width: 100%; accent-color: var(--rose-fonce); }
  .ov-toggles { display: flex; flex-wrap: wrap; gap: 12px 20px; }
  @media (pointer: coarse) { .corners .v-chip { height: 44px; } }
</style>
