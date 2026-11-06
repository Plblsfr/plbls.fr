<script lang="ts">
  import { prefs } from '../lib/prefs.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'

  const o = prefs.camOverlay
  const changed = () => { prefs.save(); void voice.applyOverlay() }
  const live = () => prefs.save()
  const corner = (x: number, y: number) => { o.x = x; o.y = y; prefs.save() }
</script>

<h3 class="set-title">Incrustation caméra <span class="plbls-badge">ALPHA</span></h3>
<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" bind:checked={o.enabled} onchange={changed} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Incruster ma caméra dans mon partage d'écran</span>
</label>
{#if o.enabled}
  <p class="small muted">Calculée sur votre appareil avant l'envoi : les spectateurs la voient sans rien régler. Active quand la caméra et le partage (sélecteur du système) sont en cours.</p>
  {#await import('./CameraOverlayPreview.svelte') then { default: Preview }}<Preview />{/await}
  <div class="plbls-field">
    <span class="plbls-field__label">Position (ou glisser dans l'aperçu)</span>
    <div class="corners">
      <button class="plbls-button plbls-button--contour" onclick={() => corner(0, 0)}>Haut gauche</button>
      <button class="plbls-button plbls-button--contour" onclick={() => corner(1, 0)}>Haut droite</button>
      <button class="plbls-button plbls-button--contour" onclick={() => corner(0, 1)}>Bas gauche</button>
      <button class="plbls-button plbls-button--contour" onclick={() => corner(1, 1)}>Bas droite</button>
    </div>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="ovsize">Taille : {Math.round(o.size * 100)} % de la largeur</label>
    <input id="ovsize" type="range" min="0.1" max="0.5" step="0.01" bind:value={o.size} onchange={live} />
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="ovshape">Forme</label>
    <select id="ovshape" class="plbls-select" bind:value={o.shape} onchange={live}>
      <option value="rect">Rectangle arrondi</option>
      <option value="circle">Cercle</option>
      <option value="free" disabled={prefs.cameraFx !== 'cutout'}>Détourage libre (avec « Fond supprimé »)</option>
    </select>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="ovmargin">Marge : {o.margin} px</label>
    <input id="ovmargin" type="range" min="0" max="120" step="4" bind:value={o.margin} onchange={live} />
  </div>
  {#each [['border', 'Bordure'], ['shadow', 'Ombre'], ['mirror', 'Miroir']] as const as [k, label] (k)}
    <label class="plbls-toggle">
      <input class="plbls-toggle__input" type="checkbox" bind:checked={o[k]} onchange={live} />
      <span class="plbls-toggle__piste"></span>
      <span class="plbls-toggle__texte">{label}</span>
    </label>
  {/each}
{/if}

<style>
  .corners { display: flex; flex-wrap: wrap; gap: 0.5rem; }
</style>
