<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../lib/api'
  import { BACKGROUNDS, FX_LABEL, prepareBackground } from '../lib/camerafx'
  import { prefs, type CameraFx } from '../lib/prefs.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'
  import Icon from './Icon.svelte'

  let error = $state('')
  const changed = () => { prefs.save(); void voice.applyFx() }
  const toDataUrl = (b: Blob) => new Promise<string>((ok, ko) => { const r = new FileReader(); r.onload = () => ok(r.result as string); r.onerror = ko; r.readAsDataURL(b) })

  // L'image perso enregistrée sur le serveur est retrouvée sur un nouvel appareil.
  onMount(() => {
    if (!prefs.customBg) api.cameraBackground().then(toDataUrl).then((u) => { prefs.customBg = u; prefs.save() }).catch(() => {})
  })

  async function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    error = ''
    try {
      const img = await prepareBackground(file)
      await api.setCameraBackground(img)
      prefs.customBg = await toDataUrl(img)
      prefs.cameraBg = 'custom'
      prefs.cameraFx = 'image'
      changed()
    } catch (err) {
      error = err instanceof Error ? err.message : 'Image refusée.'
    }
  }
  async function removeCustom() {
    await api.deleteCameraBackground().catch(() => {})
    prefs.customBg = ''
    if (prefs.cameraBg === 'custom') prefs.cameraBg = BACKGROUNDS[0].id
    changed()
  }
</script>

<fieldset class="cfx">
  <legend class="plbls-field__label">Arrière-plan de la caméra</legend>
  <div class="cfx-tiles">
    {#each Object.keys(FX_LABEL) as f (f)}
      <label class="cfx-tile" class:on={prefs.cameraFx === f}>
        <input type="radio" name="cfx" value={f as CameraFx} bind:group={prefs.cameraFx} onchange={changed} />
        <span class="cfx-swatch cfx-{f}" aria-hidden="true">{#if f === 'none'}<Icon name="interdit" size={18} />{/if}</span>
        <span class="cfx-name">{FX_LABEL[f as CameraFx]}</span>
      </label>
    {/each}
  </div>
  {#if prefs.cameraFx !== 'none'}<p class="v-note">Traité sur ton appareil, sans envoi d'image à un serveur. Les autres personnes le voient sans rien régler.</p>{/if}
</fieldset>
{#if prefs.cameraFx === 'blur'}
  <div class="plbls-field cfx-range">
    <label class="plbls-field__label" for="cblur"><span>Intensité du flou :</span> <span class="v-mono">{prefs.cameraBlur}</span></label>
    <input id="cblur" type="range" min="2" max="30" step="1" bind:value={prefs.cameraBlur} onchange={changed} />
  </div>
{/if}
{#if prefs.cameraFx === 'image'}
  <fieldset class="cfx">
    <legend class="plbls-field__label">Fond</legend>
    <div class="cfx-tiles">
      {#each BACKGROUNDS as b (b.id)}
        <label class="cfx-tile" class:on={prefs.cameraBg === b.id}>
          <input type="radio" name="cbg" value={b.id} bind:group={prefs.cameraBg} onchange={changed} />
          <span class="cfx-swatch" style="background: linear-gradient(135deg, {b.colors[0]}, {b.colors[1]})" aria-hidden="true"></span>
          <span class="cfx-name">{b.label}</span>
        </label>
      {/each}
      {#if prefs.customBg}
        <label class="cfx-tile" class:on={prefs.cameraBg === 'custom'}>
          <input type="radio" name="cbg" value="custom" bind:group={prefs.cameraBg} onchange={changed} />
          <span class="cfx-swatch" style="background-image: url({prefs.customBg})" aria-hidden="true"></span>
          <span class="cfx-name">Mon image</span>
        </label>
      {/if}
    </div>
    <div class="cfx-up">
      <label class="plbls-button plbls-button--compact plbls-button--contour cfx-file"><Icon name="image" size={14} /> Envoyer une image<input type="file" accept="image/png,image/jpeg,image/webp" aria-label="Envoyer une image de fond" onchange={upload} /></label>
      {#if prefs.customBg}<button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" onclick={removeCustom}>Supprimer mon image</button>{/if}
    </div>
    {#if error}<p class="v-alert dgr cfx-err" role="alert">{error}</p>{/if}
  </fieldset>
{/if}

<style>
  .cfx { display: grid; gap: 8px; min-width: 0; margin: 0; padding: 0; border: 0; }
  .cfx legend { padding: 0; margin-bottom: 8px; }
  .cfx-tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: 8px; }
  .cfx-tile { position: relative; display: flex; flex-direction: column; align-items: center; gap: 4px; cursor: pointer; font-size: 12px; }
  .cfx-tile input { position: absolute; opacity: 0; width: 1px; height: 1px; }
  .cfx-swatch { display: grid; place-items: center; width: 100%; height: 48px; border-radius: var(--r-s); background: var(--papier-2); background-size: cover; background-position: center; color: var(--gris); box-shadow: inset 0 0 0 1px var(--trait); transition: box-shadow var(--d) var(--sortie); }
  .cfx-none { background: repeating-linear-gradient(135deg, var(--papier-2) 0 8px, var(--papier-3) 8px 16px); }
  .cfx-blur { background: linear-gradient(135deg, var(--papier-3), var(--gris-2)); filter: blur(0.5px); }
  .cfx-cutout { background: var(--papier-3); }
  .cfx-image { background: linear-gradient(180deg, var(--rose), var(--av-4)); }
  .cfx-tile.on .cfx-swatch { box-shadow: 0 0 0 2px var(--encre); }
  .cfx-tile.on .cfx-name { font-weight: 700; }
  .cfx-tile:has(input:focus-visible) .cfx-swatch { box-shadow: var(--anneau); }
  .cfx-name { color: var(--encre); text-align: center; }
  .cfx-range label { display: flex; justify-content: space-between; }
  .cfx-range input { width: 100%; accent-color: var(--rose-fonce); }
  .cfx-up { display: flex; flex-wrap: wrap; gap: 8px; }
  .cfx-file { position: relative; cursor: pointer; }
  .cfx-file input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .cfx-file:has(input:focus-visible) { box-shadow: var(--anneau); }
  .cfx-err { margin: 0; }
</style>
